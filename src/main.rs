use std::{
    env,
    io::{self, Stdout},
    time::Duration,
};

use a6i::{
    app::{Action, App, ListView, LoadState, Mode, Service, View, valid_region},
    aws::{VerifiedContext, load_context},
    links::Arn,
    model::{Resource, ResourceLink},
    reads,
};
use aws_types::SdkConfig;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use tokio::{sync::mpsc, task::JoinHandle};

const USAGE: &str = "Usage: a6i [--profile NAME] [--region REGION]\n\nOptions:\n  --profile NAME  AWS named profile (default: SDK credential chain)\n  --region REGION  AWS region (default: SDK region chain)\n  -h, --help       Show this help\n\nReads: STS, Step Functions, Lambda, Route 53, API Gateway, CloudWatch, CloudWatch Logs, SNS, SQS, EventBridge (approved read-only operations; see README). AWS read calls may incur charges.\n";
const HELP: &str = "Navigate: Up/Down or j/k select; Enter open; Esc back\nDetails: PgUp/PgDn scroll\nLists: / filter loaded rows; r refresh\nLogs: n next log-events page\nCommands: type :name then Enter\n:route53  :apigateway  :lambda  :stepfunctions\n:cloudwatch  :sns  :sqs  :eventbridge\n:profile NAME   :region REGION\n:exit   :quit\n? or Esc closes help";

fn footer_hint(app: &App) -> &'static str {
    match &app.mode {
        Mode::Help => "? or Esc close help",
        Mode::Command(_) => "Enter run  Esc cancel",
        Mode::Search { .. } => "Enter keep filter  Esc undo",
        Mode::ConfirmRegion(_) => "Enter confirm region  Esc cancel",
        Mode::Normal => match &app.view {
            View::Selector => "j/k select  Enter open  : command  ? help",
            View::List(view)
                if view
                    .scope
                    .as_ref()
                    .is_some_and(|scope| scope.kind == "log_events")
                    && view.cursor.is_some()
                    && view.load == LoadState::Partial =>
            {
                "n next page  / filter  Enter open  Esc back  ? help"
            }
            View::List(_) => "j/k select  Enter open  / filter  Esc back  ? help",
            View::Detail { .. } => "PgUp/PgDn scroll  j/k links  Enter  Esc  ? help",
        },
    }
}

fn list_preview(view: &ListView) -> String {
    view.selected_row()
        .map(|resource| {
            resource
                .display
                .iter()
                .map(|(key, value)| format!("{}: {}", safe_line(key), safe_line(value)))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| {
            if !view.filter.is_empty() && !view.rows.is_empty() {
                "No matches in loaded rows".into()
            } else if matches!(view.load, LoadState::Complete) {
                "(empty)".into()
            } else {
                String::new()
            }
        })
}

fn safe_text(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character == '\n' {
                "\n".chars().collect::<Vec<_>>()
            } else if character.is_control() {
                character.escape_default().collect()
            } else {
                vec![character]
            }
        })
        .collect()
}

fn safe_line(text: &str) -> String {
    safe_text(text).replace('\n', "\\n")
}

enum ReadMessage {
    Page(u64, u64, Vec<Resource>, bool, Option<String>),
    Detail(u64, u64, Resource),
    Error(u64, u64, String),
}

fn start_read(
    app: &mut App,
    config: &SdkConfig,
    sender: &mpsc::Sender<ReadMessage>,
) -> Option<JoinHandle<()>> {
    let generation = app.generation;
    let view_generation = app.view_generation;
    let config = config.clone();
    let sender = sender.clone();
    match &mut app.view {
        View::List(view) if view.load == LoadState::Idle => {
            view.load = LoadState::Loading;
            let scope = view.scope.clone();
            let service = view.service;
            let cursor = view.cursor.clone();
            Some(tokio::spawn(async move {
                let log_events = scope
                    .as_ref()
                    .is_some_and(|scope| scope.kind == "log_events");
                let mut token = cursor;
                loop {
                    match reads::list_page(&config, scope.as_ref(), service, token.as_deref()).await
                    {
                        Ok(page) => {
                            let done = page.next_token.as_deref().is_none_or(str::is_empty);
                            if sender
                                .send(ReadMessage::Page(
                                    generation,
                                    view_generation,
                                    page.items,
                                    done,
                                    if log_events {
                                        page.next_token.clone()
                                    } else {
                                        None
                                    },
                                ))
                                .await
                                .is_err()
                                || done
                                || log_events
                            {
                                break;
                            }
                            token = page.next_token;
                        }
                        Err(error) => {
                            let _ = sender
                                .send(ReadMessage::Error(generation, view_generation, error))
                                .await;
                            break;
                        }
                    }
                }
            }))
        }
        View::Detail { resource, .. } if app.detail_pending => {
            if matches!(resource.kind.as_str(), "event" | "mapping")
                || a6i::coverage::is_local_detail(&resource.kind)
            {
                app.detail_pending = false;
                return None;
            }
            let resource = resource.clone();
            let context = app.context.clone();
            app.detail_pending = false;
            Some(tokio::spawn(async move {
                let message = match reads::detail(&config, &context, resource).await {
                    Ok(resource) => ReadMessage::Detail(generation, view_generation, resource),
                    Err(error) => ReadMessage::Error(generation, view_generation, error),
                };
                let _ = sender.send(message).await;
            }))
        }
        _ => None,
    }
}

fn reset_incomplete_view(app: &mut App) {
    if let View::List(view) = &mut app.view
        && (view.load == LoadState::Loading
            || (view.load == LoadState::Partial && view.cursor.is_none()))
    {
        view.rows.clear();
        view.load = LoadState::Idle;
        view.cursor = None;
    }
}
type CliOptions = (Option<String>, Option<String>);

fn args(arguments: impl IntoIterator<Item = String>) -> Result<Option<CliOptions>, String> {
    let mut arguments = arguments.into_iter();
    let mut profile = None;
    let mut region = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--profile" | "--region" => {
                let value = arguments
                    .next()
                    .filter(|value| !value.is_empty() && !value.starts_with('-'))
                    .ok_or_else(|| format!("Missing value for {argument}"))?;
                if argument == "--profile" {
                    if profile.replace(value).is_some() {
                        return Err("Duplicate --profile".into());
                    }
                } else {
                    if !valid_region(&value) {
                        return Err("Invalid region; use a region name such as us-east-1".into());
                    }
                    if region.replace(value).is_some() {
                        return Err("Duplicate --region".into());
                    }
                }
            }
            _ => return Err(format!("Unknown option: {argument}")),
        }
    }
    Ok(Some((profile, region)))
}

struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Screen {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(io::stdout())) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn render(screen: &mut Screen, app: &App) -> io::Result<()> {
    screen.terminal.draw(|frame| {
        let area = frame.area();
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(2),
            ])
            .split(area);
        let account = app.context.account.as_deref().unwrap_or("unavailable");
        let profile = app.context.profile.as_deref().unwrap_or("default chain");
        let service = match &app.view {
            View::Selector => "services",
            View::List(view) => view.service.name(),
            View::Detail { previous, .. } => previous.service.name(),
        };
        frame.render_widget(
            Paragraph::new(safe_line(&format!(
                "account: {account}   profile: {profile}   region: {}   service: {service}",
                app.context.region
            )))
            .block(
                Block::default()
                    .title(" a6i | read only ")
                    .borders(Borders::ALL),
            ),
            parts[0],
        );
        let content = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(parts[1]);
        if matches!(app.mode, Mode::Help) {
            frame.render_widget(
                Paragraph::new(HELP).block(Block::default().title(" Help ").borders(Borders::ALL)),
                parts[1],
            );
        } else {
            match &app.view {
                View::Selector => {
                    let items: Vec<_> = Service::ALL
                        .iter()
                        .enumerate()
                        .map(|(index, service)| {
                            ListItem::new(format!(
                                "{} {}",
                                if index == app.selector { ">" } else { " " },
                                service.name()
                            ))
                        })
                        .collect();
                    frame.render_widget(
                        List::new(items)
                            .block(Block::default().title(" Services ").borders(Borders::ALL)),
                        content[0],
                    );
                    frame.render_widget(
                        Paragraph::new(
                            "Select a service; open resources with Enter. Reads load on demand.",
                        )
                        .block(Block::default().title(" Detail ").borders(Borders::ALL)),
                        content[1],
                    );
                }
                View::List(view) => {
                    let visible = view.visible();
                    let state = match &view.load {
                        LoadState::Failed(message) if !view.rows.is_empty() => {
                            format!("partial; failed: {message}")
                        }
                        LoadState::Failed(message) => format!("failed: {message}"),
                        other => format!("{other:?}"),
                    };
                    let list_height = content[0].height.saturating_sub(2) as usize;
                    let start = view.selected.saturating_sub(list_height.saturating_sub(1));
                    let items: Vec<_> = visible
                        .iter()
                        .enumerate()
                        .skip(start)
                        .take(list_height)
                        .map(|(index, row)| {
                            ListItem::new(format!(
                                "{} {}",
                                if index == view.selected { ">" } else { " " },
                                safe_line(
                                    row.display
                                        .first()
                                        .map(|(_, value)| value.as_str())
                                        .unwrap_or(&row.id)
                                )
                            ))
                        })
                        .collect();
                    let page_hint = if view.cursor.is_some() && view.load == LoadState::Partial {
                        " | n next page"
                    } else {
                        ""
                    };
                    let count = if view.filter.is_empty() {
                        format!("{} rows", view.rows.len())
                    } else {
                        format!("{}/{} rows", visible.len(), view.rows.len())
                    };
                    frame.render_widget(
                        List::new(items).block(
                            Block::default()
                                .title(format!(
                                    " {} | {count} | {state}{page_hint} ",
                                    view.service.name()
                                ))
                                .borders(Borders::ALL),
                        ),
                        content[0],
                    );
                    frame.render_widget(
                        Paragraph::new(list_preview(view))
                            .wrap(Wrap { trim: false })
                            .block(
                                Block::default()
                                    .title(" Enter: full detail ")
                                    .borders(Borders::ALL),
                            ),
                        content[1],
                    );
                }
                View::Detail {
                    previous,
                    resource,
                    link,
                } => {
                    let rows: Vec<_> = resource
                        .details
                        .iter()
                        .flat_map(|(key, value)| {
                            format!("{}: {}", safe_line(key), safe_text(value))
                                .lines()
                                .map(|line| Line::from(line.to_owned()))
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    frame.render_widget(
                        Paragraph::new(rows)
                            .wrap(Wrap { trim: false })
                            .scroll((app.scroll, 0))
                            .block(
                                Block::default()
                                    .title(format!(
                                        " {} / {} ",
                                        previous.service.name(),
                                        safe_line(&resource.id)
                                    ))
                                    .borders(Borders::ALL),
                            ),
                        content[0],
                    );
                    let link_height = content[1].height.saturating_sub(2) as usize;
                    let link_start = link.saturating_sub(link_height.saturating_sub(1));
                    let links: Vec<_> = resource
                        .links
                        .iter()
                        .enumerate()
                        .skip(link_start)
                        .take(link_height)
                        .map(|(index, target)| {
                            ListItem::new(format!(
                                "{} {}: {}",
                                if index == *link { ">" } else { " " },
                                target.kind,
                                safe_line(&target.id)
                            ))
                        })
                        .collect();
                    frame.render_widget(
                        List::new(links).block(
                            Block::default()
                                .title(" Links | j/k, Enter ")
                                .borders(Borders::ALL),
                        ),
                        content[1],
                    );
                }
            }
        }
        let breadcrumb = match &app.view {
            View::Selector => "services".to_string(),
            View::List(view) => format!(
                "services / {} / {}",
                view.service.name(),
                view.scope
                    .as_ref()
                    .map(|scope| safe_line(&scope.id))
                    .unwrap_or_default()
            ),
            View::Detail {
                previous, resource, ..
            } => format!(
                "services / {} / {}",
                previous.service.name(),
                safe_line(&resource.id)
            ),
        };
        let mode = match &app.mode {
            Mode::Normal => app.status.clone(),
            Mode::Command(text) => format!(":{text}"),
            Mode::Search { text, .. } => format!("/{text} (loaded rows only)"),
            Mode::Help => app.status.clone(),
            Mode::ConfirmRegion(_) => app.status.clone(),
        };
        frame.render_widget(
            Paragraph::new(safe_line(&mode)).block(
                Block::default()
                    .title(format!(" {breadcrumb} "))
                    .borders(Borders::ALL),
            ),
            parts[2],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(footer_hint(app)),
            ])),
            parts[3],
        );
    })?;
    Ok(())
}

async fn run() -> Result<(), String> {
    let (profile, region) = match args(env::args().skip(1)) {
        Ok(Some(values)) => values,
        Ok(None) => {
            print!("{USAGE}");
            return Ok(());
        }
        Err(message) => return Err(format!("{message}\n{USAGE}")),
    };
    if !io::IsTerminal::is_terminal(&io::stdin()) || !io::IsTerminal::is_terminal(&io::stdout()) {
        return Err("Interactive terminal required; use --help for usage".into());
    }
    let initial = load_context(profile, region).await?;
    let mut app = App::new(initial.context);
    let mut config = initial.config;
    if app.context.account.is_none() {
        app.status =
            "STS identity unavailable; check sts:GetCallerIdentity permission or network".into();
    }
    let mut screen = Screen::enter().map_err(|error| format!("Terminal setup failed: {error}"))?;
    let (sender, mut receiver) =
        mpsc::unbounded_channel::<(u64, Result<VerifiedContext, String>)>();
    let (read_sender, mut read_receiver) = mpsc::channel::<ReadMessage>(8);
    let mut reading: Option<JoinHandle<()>> = None;
    let mut pending: Option<JoinHandle<()>> = None;
    let mut pending_target: Option<ResourceLink> = None;
    let mut saved_config = None;
    let mut request_id = 0u64;
    loop {
        render(&mut screen, &app).map_err(|error| format!("Terminal render failed: {error}"))?;
        while let Ok((id, result)) = receiver.try_recv() {
            if id != request_id {
                continue;
            }
            pending = None;
            match result {
                Ok(verified) => {
                    if let Some(target) = pending_target.take() {
                        if app.enter_region_link(verified.context, target) {
                            saved_config = Some(std::mem::replace(&mut config, verified.config));
                        }
                    } else {
                        if let Some(task) = reading.take() {
                            task.abort();
                        }
                        saved_config = None;
                        config = verified.config;
                        app.replace_context(verified.context);
                        if app.context.account.is_none() {
                            app.status = "STS identity unavailable; check sts:GetCallerIdentity permission or network".into();
                        }
                    }
                }
                Err(message) => {
                    pending_target = None;
                    app.status = message;
                }
            }
        }
        for _ in 0..8 {
            let Ok(message) = read_receiver.try_recv() else {
                break;
            };
            match message {
                ReadMessage::Page(generation, view, rows, done, cursor) => {
                    app.apply_page(generation, view, rows, done);
                    app.apply_cursor(generation, view, cursor);
                }
                ReadMessage::Detail(generation, view, resource) => {
                    app.apply_detail(generation, view, resource)
                }
                ReadMessage::Error(generation, view, error) => {
                    if app.generation == generation
                        && app.view_generation == view
                        && app.linked_detail
                    {
                        if app.is_region_link_root() {
                            app.leave_region_link();
                            if let Some(original) = saved_config.take() {
                                config = original;
                            }
                            app.status = error;
                        } else {
                            app.fail_link(error);
                        }
                    } else {
                        app.apply_error(generation, view, error);
                    }
                }
            }
        }
        if reading.as_ref().is_some_and(JoinHandle::is_finished) {
            reading = None;
        }
        if reading.is_none() && pending.is_none() {
            reading = start_read(&mut app, &config, &read_sender);
        }
        if !event::poll(Duration::from_millis(50))
            .map_err(|error| format!("Terminal input failed: {error}"))?
        {
            continue;
        }
        let Event::Key(key) =
            event::read().map_err(|error| format!("Terminal input failed: {error}"))?
        else {
            continue;
        };
        let old_view = app.view_generation;
        let (next_profile, next_region) = match app.key(key) {
            Action::Quit => break,
            Action::SwitchProfile(name) => {
                pending_target = None;
                (Some(name), None)
            }
            Action::SwitchRegion(name) => {
                pending_target = None;
                (None, Some(name))
            }
            Action::FollowRegion(target) => {
                let region = Arn::parse(&target.id)
                    .expect("confirmed ARN")
                    .region
                    .to_owned();
                pending_target = Some(target);
                (None, Some(region))
            }
            Action::ReturnContext => {
                if let Some(task) = reading.take() {
                    task.abort();
                }
                if let Some(task) = pending.take() {
                    task.abort();
                }
                pending_target = None;
                request_id = request_id.wrapping_add(1);
                if let Some(original) = saved_config.take() {
                    config = original;
                }
                app.leave_region_link();
                app.status.clear();
                continue;
            }
            Action::None => {
                if old_view != app.view_generation {
                    if let Some(task) = reading.take() {
                        task.abort();
                    }
                    reset_incomplete_view(&mut app);
                }
                continue;
            }
        };
        let interrupted_detail = reading.is_some() && matches!(app.view, View::Detail { .. });
        if let Some(task) = reading.take() {
            task.abort();
        }
        app.view_generation = app.view_generation.wrapping_add(1);
        if interrupted_detail {
            app.detail_pending = true;
        }
        reset_incomplete_view(&mut app);
        if let Some(task) = pending.take() {
            task.abort();
        }
        request_id = request_id.wrapping_add(1);
        let id = request_id;
        let sender = sender.clone();
        let profile = next_profile.or_else(|| app.context.profile.clone());
        let region = next_region.or_else(|| Some(app.context.region.clone()));
        app.status = "Checking AWS context...".into();
        pending = Some(tokio::spawn(async move {
            let _ = sender.send((id, load_context(profile, region).await));
        }));
    }
    if let Some(task) = pending {
        task.abort();
    }
    if let Some(task) = reading {
        task.abort();
    }
    drop(config);
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_help_and_rejection_need_no_credentials() {
        assert!(args(["--help".into()]).unwrap().is_none());
        assert!(
            args([
                "--profile".into(),
                "work".into(),
                "--region".into(),
                "us-east-1".into()
            ])
            .unwrap()
            .is_some()
        );
        for input in [
            vec!["--unknown"],
            vec!["--region", "bad"],
            vec!["--profile"],
            vec!["--profile", "a", "--profile", "b"],
        ] {
            assert!(args(input.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn resource_text_cannot_inject_terminal_controls() {
        let value = safe_line("record\x1b]0;unsafe\x07\nnext\rline");
        assert!(!value.chars().any(char::is_control));
        assert!(value.contains("\\n"));
        assert_eq!(safe_text("first\nsecond"), "first\nsecond");
    }

    #[test]
    fn filtered_preview_distinguishes_no_match_from_empty_list() {
        let mut view = ListView::new(Service::Lambda);
        view.rows.push(Resource {
            service: Service::Lambda,
            kind: "function".into(),
            id: "worker".into(),
            display: vec![("name".into(), "worker".into())],
            details: vec![],
            links: vec![],
        });
        view.load = LoadState::Complete;
        view.set_filter("missing".into());
        assert_eq!(view.visible().len(), 0);
        assert_eq!(list_preview(&view), "No matches in loaded rows");
        view.rows.clear();
        assert_eq!(list_preview(&view), "(empty)");
    }
}

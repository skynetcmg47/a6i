use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    links::{Arn, static_link},
    model::{Resource, ResourceLink},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Route53,
    ApiGateway,
    Lambda,
    StepFunctions,
    CloudWatch,
    Sns,
    Sqs,
    EventBridge,
}

impl Service {
    pub const ALL: [Self; 8] = [
        Self::Route53,
        Self::ApiGateway,
        Self::Lambda,
        Self::StepFunctions,
        Self::CloudWatch,
        Self::Sns,
        Self::Sqs,
        Self::EventBridge,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Route53 => "route53",
            Self::ApiGateway => "apigateway",
            Self::Lambda => "lambda",
            Self::StepFunctions => "stepfunctions",
            Self::CloudWatch => "cloudwatch",
            Self::Sns => "sns",
            Self::Sqs => "sqs",
            Self::EventBridge => "eventbridge",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|service| service.name() == input)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub account: Option<String>,
    pub profile: Option<String>,
    pub region: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Select(Service),
    Profile(String),
    Region(String),
    Quit,
}

pub fn valid_region(name: &str) -> bool {
    name.len() <= 63
        && name.contains('-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub fn parse_command(input: &str) -> Result<Command, String> {
    let parts: Vec<_> = input.split_whitespace().collect();
    match parts.as_slice() {
        ["exit" | "quit"] => Ok(Command::Quit),
        [service] if Service::parse(service).is_some() => {
            Ok(Command::Select(Service::parse(service).unwrap()))
        }
        ["profile", name] => Ok(Command::Profile((*name).to_string())),
        ["region", name] if valid_region(name) => Ok(Command::Region((*name).to_string())),
        ["region", _] => Err("Invalid region; use a region name such as us-east-1".into()),
        _ => Err("Unknown command; press ? for allowed commands".into()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command(String),
    Search { text: String, previous: String },
    Help,
    ConfirmRegion(ResourceLink),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadState {
    Idle,
    Loading,
    Partial,
    Complete,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ListView {
    pub service: Service,
    pub scope: Option<ResourceLink>,
    pub rows: Vec<Resource>,
    pub filter: String,
    pub selected: usize,
    pub load: LoadState,
    pub cursor: Option<String>,
}

impl ListView {
    pub fn new(service: Service) -> Self {
        Self {
            service,
            scope: None,
            rows: Vec::new(),
            filter: String::new(),
            selected: 0,
            load: LoadState::Idle,
            cursor: None,
        }
    }

    pub fn visible(&self) -> Vec<&Resource> {
        let needle = self.filter.to_lowercase();
        self.rows
            .iter()
            .filter(|row| {
                row.id.to_lowercase().contains(&needle)
                    || row.display.iter().any(|(key, value)| {
                        key.to_lowercase().contains(&needle)
                            || value.to_lowercase().contains(&needle)
                    })
            })
            .collect()
    }

    pub fn selected_row(&self) -> Option<&Resource> {
        self.visible().get(self.selected).copied()
    }

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }
}

#[derive(Clone, Debug)]
pub enum View {
    Selector,
    List(ListView),
    Detail {
        previous: ListView,
        resource: Resource,
        link: usize,
    },
}

pub enum Action {
    None,
    Quit,
    SwitchProfile(String),
    SwitchRegion(String),
    FollowRegion(ResourceLink),
    ReturnContext,
}

struct SavedContext {
    context: Context,
    view: View,
    history: Vec<View>,
    linked_detail: bool,
    scroll: u16,
}

pub struct App {
    pub context: Context,
    pub generation: u64,
    pub view_generation: u64,
    pub mode: Mode,
    pub view: View,
    pub selector: usize,
    pub status: String,
    pub history: Vec<View>,
    pub detail_pending: bool,
    pub scroll: u16,
    pub linked_detail: bool,
    return_context: Option<Box<SavedContext>>,
}

impl App {
    pub fn new(context: Context) -> Self {
        Self {
            context,
            generation: 0,
            view_generation: 0,
            mode: Mode::Normal,
            view: View::Selector,
            selector: 0,
            status: String::new(),
            history: Vec::new(),
            detail_pending: false,
            scroll: 0,
            linked_detail: false,
            return_context: None,
        }
    }

    pub fn replace_context(&mut self, context: Context) {
        self.context = context;
        self.generation = self.generation.wrapping_add(1);
        self.view_generation = self.view_generation.wrapping_add(1);
        self.view = View::Selector;
        self.history.clear();
        self.return_context = None;
        self.linked_detail = false;
        self.mode = Mode::Normal;
        self.status = "Context updated; resource state cleared".into();
    }

    pub fn select(&mut self, service: Service) {
        self.view_generation = self.view_generation.wrapping_add(1);
        self.view = View::List(ListView::new(service));
        self.history.clear();
        self.linked_detail = false;
        self.selector = Service::ALL
            .iter()
            .position(|item| *item == service)
            .unwrap();
        self.status = "Loading...".into();
    }

    pub fn apply_page(
        &mut self,
        generation: u64,
        view_generation: u64,
        rows: Vec<Resource>,
        done: bool,
    ) {
        if self.generation != generation || self.view_generation != view_generation {
            return;
        }
        if let View::List(view) = &mut self.view {
            view.rows.extend(rows);
            view.load = if done {
                LoadState::Complete
            } else {
                LoadState::Partial
            };
            if self.status == "Loading..." {
                self.status.clear();
            }
        }
    }

    pub fn apply_cursor(&mut self, generation: u64, view_generation: u64, cursor: Option<String>) {
        if self.generation == generation
            && self.view_generation == view_generation
            && let View::List(view) = &mut self.view
        {
            view.cursor = cursor;
        }
    }

    pub fn apply_error(&mut self, generation: u64, view_generation: u64, message: String) {
        if self.generation != generation || self.view_generation != view_generation {
            return;
        }
        if let View::List(view) = &mut self.view {
            view.load = LoadState::Failed(message);
            if self.status == "Loading..." {
                self.status.clear();
            }
        } else {
            self.detail_pending = false;
            self.status = message;
        }
    }

    pub fn apply_detail(&mut self, generation: u64, view_generation: u64, resource: Resource) {
        if self.generation != generation || self.view_generation != view_generation {
            return;
        }
        if let View::Detail {
            resource: current, ..
        } = &mut self.view
        {
            *current = resource;
            self.detail_pending = false;
            self.scroll = 0;
            self.status.clear();
        }
    }

    pub fn open_link(&mut self, target: ResourceLink) {
        if matches!(
            target.kind.as_str(),
            "function" | "machine" | "topic" | "queue"
        ) && target.id.starts_with("arn:")
        {
            let Some(arn) = Arn::parse(&target.id) else {
                self.status = "Invalid target ARN".into();
                return;
            };
            if static_link(&target.id).as_ref() != Some(&target) {
                self.status = "Unsupported target ARN".into();
                return;
            }
            if self.context.account.as_deref() != Some(arn.account) {
                self.status =
                    "Target account differs or identity unavailable; switch profile explicitly"
                        .into();
                return;
            }
            let partition = if self.context.region.starts_with("cn-") {
                "aws-cn"
            } else if self.context.region.starts_with("us-gov-") {
                "aws-us-gov"
            } else {
                "aws"
            };
            if arn.partition != partition {
                self.status = "Target partition differs; switch profile explicitly".into();
                return;
            }
            if arn.region != self.context.region {
                if self.return_context.is_some() {
                    self.status = "Return to source context before another region switch".into();
                    return;
                }
                self.status = format!(
                    "Switch to {} to open target? Enter confirms, Esc cancels",
                    arn.region
                );
                self.mode = Mode::ConfirmRegion(target);
                return;
            }
        }
        let View::Detail { previous, .. } = &self.view else {
            return;
        };
        self.history.push(self.view.clone());
        self.view_generation = self.view_generation.wrapping_add(1);
        if matches!(target.kind.as_str(), "executions" | "history" | "mappings")
            || crate::coverage::is_list_kind(&target.kind)
        {
            let mut list = ListView::new(target.service);
            list.scope = Some(target);
            self.view = View::List(list);
            self.linked_detail = false;
        } else {
            let source = previous.clone();
            self.view = View::Detail {
                previous: source,
                resource: Resource {
                    service: target.service,
                    kind: target.kind,
                    id: target.id,
                    display: vec![],
                    details: vec![],
                    links: vec![],
                },
                link: 0,
            };
            self.detail_pending = true;
            self.linked_detail = true;
        }
        self.scroll = 0;
    }

    pub fn enter_region_link(&mut self, context: Context, target: ResourceLink) -> bool {
        if context.account.is_none()
            || context.account != self.context.account
            || static_link(&target.id).as_ref() != Some(&target)
            || Arn::parse(&target.id).is_none_or(|arn| arn.region != context.region)
        {
            self.status = "Target account could not be verified; switch profile explicitly".into();
            return false;
        }
        let snapshot = SavedContext {
            context: self.context.clone(),
            view: self.view.clone(),
            history: std::mem::take(&mut self.history),
            linked_detail: self.linked_detail,
            scroll: self.scroll,
        };
        self.return_context = Some(Box::new(snapshot));
        self.context = context;
        self.generation = self.generation.wrapping_add(1);
        self.view_generation = self.view_generation.wrapping_add(1);
        self.mode = Mode::Normal;
        self.status = "Loading target...".into();
        let previous = ListView::new(target.service);
        self.view = View::Detail {
            previous,
            resource: Resource {
                service: target.service,
                kind: target.kind,
                id: target.id,
                display: vec![],
                details: vec![],
                links: vec![],
            },
            link: 0,
        };
        self.linked_detail = true;
        self.detail_pending = true;
        self.scroll = 0;
        true
    }

    pub fn leave_region_link(&mut self) {
        if let Some(saved) = self.return_context.take() {
            self.context = saved.context;
            self.view = saved.view;
            self.history = saved.history;
            self.linked_detail = saved.linked_detail;
            self.scroll = saved.scroll;
            self.detail_pending = false;
            self.generation = self.generation.wrapping_add(1);
            self.view_generation = self.view_generation.wrapping_add(1);
        }
    }

    pub fn is_region_link_root(&self) -> bool {
        self.return_context.is_some() && self.history.is_empty()
    }

    pub fn fail_link(&mut self, message: String) {
        if self.linked_detail {
            self.view_generation = self.view_generation.wrapping_add(1);
            self.view = self.history.pop().expect("linked detail has source");
            self.linked_detail = false;
            self.detail_pending = false;
        }
        self.status = message;
    }

    pub fn key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }
        match &mut self.mode {
            Mode::Command(text) => match key.code {
                KeyCode::Esc => self.mode = Mode::Normal,
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text.push(character)
                }
                KeyCode::Enter => {
                    let command = parse_command(text);
                    self.mode = Mode::Normal;
                    match command {
                        Ok(Command::Quit) => return Action::Quit,
                        Ok(Command::Select(service)) => self.select(service),
                        Ok(Command::Profile(name)) => return Action::SwitchProfile(name),
                        Ok(Command::Region(name)) => return Action::SwitchRegion(name),
                        Err(message) => self.status = message,
                    }
                }
                _ => {}
            },
            Mode::Search { text, previous } => match key.code {
                KeyCode::Esc => {
                    if let View::List(view) = &mut self.view {
                        view.set_filter(previous.clone());
                    }
                    self.mode = Mode::Normal;
                }
                KeyCode::Enter => self.mode = Mode::Normal,
                KeyCode::Backspace => {
                    text.pop();
                    if let View::List(view) = &mut self.view {
                        view.set_filter(text.clone());
                    }
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text.push(character);
                    if let View::List(view) = &mut self.view {
                        view.set_filter(text.clone());
                    }
                }
                _ => {}
            },
            Mode::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                    self.mode = Mode::Normal;
                }
            }
            Mode::ConfirmRegion(target) => match key.code {
                KeyCode::Enter => {
                    let target = target.clone();
                    self.mode = Mode::Normal;
                    return Action::FollowRegion(target);
                }
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.status.clear();
                }
                _ => {}
            },
            Mode::Normal => match key.code {
                KeyCode::Char(':') => self.mode = Mode::Command(String::new()),
                KeyCode::Char('/') if matches!(self.view, View::List(_)) => {
                    if let View::List(view) = &self.view {
                        self.mode = Mode::Search {
                            text: view.filter.clone(),
                            previous: view.filter.clone(),
                        };
                    }
                }
                KeyCode::Char('?') => self.mode = Mode::Help,
                KeyCode::Char('n') => {
                    if let View::List(view) = &mut self.view
                        && view
                            .scope
                            .as_ref()
                            .is_some_and(|scope| scope.kind == "log_events")
                        && view.cursor.is_some()
                        && view.load == LoadState::Partial
                    {
                        view.load = LoadState::Idle;
                    }
                }
                KeyCode::Char('r') => {
                    if let View::List(view) = &mut self.view {
                        self.view_generation = self.view_generation.wrapping_add(1);
                        view.rows.clear();
                        view.selected = 0;
                        view.load = LoadState::Idle;
                        view.cursor = None;
                    } else if matches!(self.view, View::Detail { .. }) {
                        self.view_generation = self.view_generation.wrapping_add(1);
                        self.detail_pending = true;
                    }
                }
                KeyCode::Esc if self.is_region_link_root() => return Action::ReturnContext,
                KeyCode::Esc
                    if !self.history.is_empty()
                        && (!matches!(self.view, View::Detail { .. }) || self.linked_detail) =>
                {
                    self.view_generation = self.view_generation.wrapping_add(1);
                    self.view = self.history.pop().unwrap();
                    self.detail_pending = false;
                    self.linked_detail = false;
                    self.scroll = 0;
                }
                KeyCode::Esc => match &self.view {
                    View::Detail { previous, .. } => {
                        self.view_generation = self.view_generation.wrapping_add(1);
                        self.view = View::List(previous.clone());
                    }
                    View::List(_) => {
                        self.view_generation = self.view_generation.wrapping_add(1);
                        self.view = View::Selector;
                    }
                    View::Selector => {}
                },
                KeyCode::Enter => match &self.view {
                    View::Selector => self.select(Service::ALL[self.selector]),
                    View::List(view) => {
                        if let Some(resource) = view.selected_row() {
                            self.view_generation = self.view_generation.wrapping_add(1);
                            self.view = View::Detail {
                                previous: view.clone(),
                                resource: resource.clone(),
                                link: 0,
                            };
                            self.detail_pending = true;
                            self.linked_detail = false;
                            self.scroll = 0;
                        }
                    }
                    View::Detail { resource, link, .. } => {
                        if let Some(target) = resource.links.get(*link).cloned() {
                            self.open_link(target);
                        }
                    }
                },
                KeyCode::PageDown if matches!(self.view, View::Detail { .. }) => {
                    self.scroll = self.scroll.saturating_add(15)
                }
                KeyCode::PageUp if matches!(self.view, View::Detail { .. }) => {
                    self.scroll = self.scroll.saturating_sub(15)
                }
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(false),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(true),
                _ => {}
            },
        }
        Action::None
    }

    fn move_selection(&mut self, down: bool) {
        match &mut self.view {
            View::Selector => self.selector = step(self.selector, Service::ALL.len(), down),
            View::List(view) => view.selected = step(view.selected, view.visible().len(), down),
            View::Detail { resource, link, .. } => *link = step(*link, resource.links.len(), down),
        }
    }
}

fn step(index: usize, len: usize, down: bool) -> usize {
    if down {
        (index + 1).min(len.saturating_sub(1))
    } else {
        index.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(Context {
            account: Some("123".into()),
            profile: None,
            region: "us-east-1".into(),
        })
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn resource(id: &str) -> Resource {
        Resource {
            service: Service::Lambda,
            kind: "function".into(),
            id: id.into(),
            display: vec![],
            details: vec![],
            links: vec![],
        }
    }

    #[test]
    fn whitelist_rejects_extra_tokens_and_invalid_regions() {
        assert!(matches!(
            parse_command("lambda"),
            Ok(Command::Select(Service::Lambda))
        ));
        assert!(matches!(
            parse_command("profile work"),
            Ok(Command::Profile(_))
        ));
        assert!(matches!(
            parse_command("region us-east-1"),
            Ok(Command::Region(_))
        ));
        assert!(matches!(parse_command("exit"), Ok(Command::Quit)));
        assert!(matches!(parse_command("quit"), Ok(Command::Quit)));
        for bad in [
            "lambda extra",
            "profile",
            "region INVALID",
            "region us-east-1 extra",
            "!sh",
            "get-caller-identity",
            "quit now",
            "q",
        ] {
            assert!(parse_command(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn typing_does_not_trigger_normal_bindings() {
        let mut app = app();
        app.key(key(KeyCode::Char(':')));
        for character in "route53".chars() {
            app.key(key(KeyCode::Char(character)));
        }
        assert!(matches!(app.mode, Mode::Command(_)));
        app.key(key(KeyCode::Enter));
        assert!(matches!(app.view, View::List(_)));
        app.key(key(KeyCode::Char('/')));
        app.key(key(KeyCode::Char('q')));
        assert!(matches!(app.mode, Mode::Search { .. }));
    }

    #[test]
    fn help_closes_without_changing_view_or_status() {
        let mut app = app();
        app.select(Service::Lambda);
        app.status = "Access denied".into();
        app.key(key(KeyCode::Char('?')));
        assert!(matches!(app.mode, Mode::Help));
        app.key(key(KeyCode::Esc));
        assert!(matches!(app.mode, Mode::Normal));
        assert!(matches!(app.view, View::List(_)));
        assert_eq!(app.status, "Access denied");
        app.key(key(KeyCode::Char('?')));
        app.key(key(KeyCode::Char('?')));
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.status, "Access denied");
    }

    #[test]
    fn stale_pages_and_errors_cannot_change_context() {
        let mut app = app();
        app.select(Service::Lambda);
        let old_view = app.view_generation;
        app.replace_context(Context {
            account: None,
            profile: Some("next".into()),
            region: "eu-west-1".into(),
        });
        app.select(Service::Lambda);
        app.apply_page(0, old_view, vec![resource("old")], true);
        app.apply_error(0, old_view, "old error".into());
        if let View::List(view) = &app.view {
            assert!(view.rows.is_empty());
            assert_eq!(view.load, LoadState::Idle);
        } else {
            panic!()
        }
    }

    #[test]
    fn filter_is_local_and_back_preserves_selection() {
        let mut app = app();
        app.select(Service::Lambda);
        app.apply_page(
            0,
            app.view_generation,
            vec![resource("Alpha"), resource("beta")],
            false,
        );
        app.key(key(KeyCode::Char('/')));
        app.key(key(KeyCode::Char('B')));
        app.key(key(KeyCode::Enter));
        if let View::List(view) = &app.view {
            assert_eq!(view.visible().len(), 1);
            assert_eq!(view.load, LoadState::Partial);
        }
        app.key(key(KeyCode::Enter));
        assert!(matches!(app.view, View::Detail { .. }));
        let old_view = app.view_generation;
        app.key(key(KeyCode::Esc));
        app.apply_page(0, old_view, vec![resource("stale")], true);
        if let View::List(view) = &app.view {
            assert_eq!(view.filter, "B");
            assert_eq!(view.selected_row().unwrap().id, "beta");
            assert_eq!(view.rows.len(), 2);
        } else {
            panic!()
        }
    }

    #[test]
    fn errors_preserve_rows_and_escape_stays_at_root() {
        let mut app = app();
        app.select(Service::Lambda);
        app.apply_page(0, app.view_generation, vec![resource("kept")], false);
        app.apply_error(0, app.view_generation, "Access denied".into());
        if let View::List(view) = &app.view {
            assert_eq!(view.rows.len(), 1);
            assert_eq!(view.load, LoadState::Failed("Access denied".into()));
        } else {
            panic!()
        }
        app.key(key(KeyCode::Esc));
        assert!(matches!(app.view, View::Selector));
        assert!(matches!(app.key(key(KeyCode::Esc)), Action::None));
        assert!(matches!(app.key(key(KeyCode::Char('q'))), Action::None));
    }

    #[test]
    fn only_typed_exit_or_quit_closes_app() {
        for command in ["exit", "quit"] {
            let mut app = app();
            app.select(Service::Lambda);
            app.key(key(KeyCode::Char(':')));
            for character in command.chars() {
                assert!(matches!(
                    app.key(key(KeyCode::Char(character))),
                    Action::None
                ));
            }
            assert!(matches!(app.key(key(KeyCode::Enter)), Action::Quit));
        }
    }

    #[test]
    fn link_failure_restores_filtered_machine_and_rejects_wrong_context() {
        let mut app = app();
        app.select(Service::StepFunctions);
        app.apply_page(0, app.view_generation, vec![resource("machine")], true);
        if let View::List(list) = &mut app.view {
            list.set_filter("machine".into());
        }
        app.key(key(KeyCode::Enter));
        let target = ResourceLink {
            service: Service::Lambda,
            kind: "function".into(),
            id: "arn:aws:lambda:us-east-1:123:function:ok".into(),
        };
        app.open_link(ResourceLink {
            id: target.id.replace(":123:", ":999:"),
            ..target.clone()
        });
        assert!(matches!(app.view, View::Detail { ref resource, .. } if resource.id == "machine"));
        app.open_link(target);
        assert!(app.linked_detail);
        app.fail_link("AWS access denied".into());
        assert!(matches!(app.view, View::Detail { ref resource, .. } if resource.id == "machine"));
        app.key(key(KeyCode::Esc));
        assert!(
            matches!(app.view, View::List(ref list) if list.filter == "machine" && list.selected == 0)
        );
    }

    #[test]
    fn region_link_requires_confirmation_and_restores_source_and_stale_reads() {
        let mut app = app();
        app.select(Service::StepFunctions);
        app.apply_page(0, app.view_generation, vec![resource("machine")], true);
        if let View::List(list) = &mut app.view {
            list.set_filter("machine".into());
        }
        app.key(key(KeyCode::Enter));
        let target = ResourceLink {
            service: Service::Lambda,
            kind: "function".into(),
            id: "arn:aws:lambda:eu-west-1:123:function:worker".into(),
        };
        app.open_link(target.clone());
        assert!(matches!(app.mode, Mode::ConfirmRegion(_)));
        app.key(key(KeyCode::Esc));
        assert!(matches!(app.mode, Mode::Normal));
        app.open_link(target.clone());
        assert!(matches!(
            app.key(key(KeyCode::Enter)),
            Action::FollowRegion(_)
        ));
        let stale_generation = app.generation;
        let stale_view = app.view_generation;
        assert!(!app.enter_region_link(
            Context {
                account: Some("999".into()),
                profile: None,
                region: "eu-west-1".into(),
            },
            target.clone(),
        ));
        assert_eq!(app.context.region, "us-east-1");
        assert!(app.enter_region_link(
            Context {
                account: Some("123".into()),
                profile: None,
                region: "eu-west-1".into(),
            },
            target.clone(),
        ));
        app.apply_detail(stale_generation, stale_view, resource("stale"));
        assert!(matches!(&app.view, View::Detail { resource, .. } if resource.id == target.id));
        assert!(matches!(app.key(key(KeyCode::Esc)), Action::ReturnContext));
        app.leave_region_link();
        assert_eq!(app.context.region, "us-east-1");
        assert!(matches!(app.view, View::Detail { ref resource, .. } if resource.id == "machine"));
        app.key(key(KeyCode::Esc));
        assert!(
            matches!(app.view, View::List(ref list) if list.filter == "machine" && list.selected == 0)
        );
    }

    #[test]
    fn account_and_partition_refusals_do_not_change_view() {
        let mut app = app();
        app.select(Service::Sns);
        app.view = View::Detail {
            previous: ListView::new(Service::Sns),
            resource: resource("source"),
            link: 0,
        };
        let target = ResourceLink {
            service: Service::Sns,
            kind: "topic".into(),
            id: "arn:aws:sns:us-east-1:999:topic".into(),
        };
        app.open_link(target.clone());
        assert!(app.status.contains("switch profile"));
        app.context.account = None;
        app.open_link(ResourceLink {
            id: target.id.replace(":999:", ":123:"),
            ..target
        });
        assert!(app.status.contains("identity unavailable"));
        assert!(matches!(app.view, View::Detail { ref resource, .. } if resource.id == "source"));
    }

    #[test]
    fn log_event_pages_load_only_on_request_and_clear_on_context_change() {
        let mut app = app();
        app.select(Service::CloudWatch);
        app.view = View::List(ListView {
            scope: Some(ResourceLink {
                service: Service::CloudWatch,
                kind: "log_events".into(),
                id: "group/stream".into(),
            }),
            ..ListView::new(Service::CloudWatch)
        });
        let generation = app.view_generation;
        app.apply_page(0, generation, vec![resource("one")], false);
        app.apply_cursor(0, generation, Some("next".into()));
        assert!(matches!(&app.view, View::List(view) if view.load == LoadState::Partial));
        app.key(key(KeyCode::Char('n')));
        assert!(
            matches!(&app.view, View::List(view) if view.load == LoadState::Idle && view.rows.len() == 1)
        );
        app.apply_page(0, generation, vec![resource("two")], true);
        app.apply_cursor(0, generation, None);
        assert!(
            matches!(&app.view, View::List(view) if view.load == LoadState::Complete && view.rows.len() == 2)
        );
        app.replace_context(Context {
            account: Some("456".into()),
            profile: None,
            region: "eu-west-1".into(),
        });
        app.apply_page(0, generation, vec![resource("stale")], true);
        assert!(matches!(app.view, View::Selector));
    }
}

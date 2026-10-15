use aws_types::SdkConfig;

use crate::{
    app::{Context, Service},
    asl::lambda_links,
    links::add_link,
    model::{Page, Resource, ResourceLink},
};

fn row(service: Service, kind: &str, id: &str, display: Vec<(String, String)>) -> Resource {
    Resource {
        service,
        kind: kind.into(),
        id: id.into(),
        display,
        details: vec![],
        links: vec![],
    }
}

fn field(key: &str, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

fn readable_json(value: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
        .unwrap_or_else(|| value.to_owned())
}

fn failure(error: impl std::fmt::Display) -> String {
    let message = error.to_string().to_lowercase();
    if message.contains("accessdenied") || message.contains("access denied") {
        "AWS access denied; check read permissions".into()
    } else if message.contains("doesnotexist") || message.contains("resourcenotfound") {
        "AWS resource not found; refresh the source view".into()
    } else if message.contains("throttl") || message.contains("toomanyrequests") {
        "AWS request throttled; retry later".into()
    } else {
        "AWS read failed; check network, region and permissions".into()
    }
}

pub async fn list_page(
    config: &SdkConfig,
    scope: Option<&ResourceLink>,
    service: Service,
    token: Option<&str>,
) -> Result<Page<Resource>, String> {
    match (service, scope.map(|link| link.kind.as_str())) {
        (Service::StepFunctions, None) => {
            let response = aws_sdk_sfn::Client::new(config)
                .list_state_machines()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .state_machines()
                    .iter()
                    .map(|machine| {
                        row(
                            service,
                            "machine",
                            machine.state_machine_arn(),
                            vec![
                                field("name", machine.name()),
                                field("type", machine.r#type()),
                            ],
                        )
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::StepFunctions, Some("executions")) => {
            let response = aws_sdk_sfn::Client::new(config)
                .list_executions()
                .state_machine_arn(&scope.unwrap().id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .executions()
                    .iter()
                    .map(|execution| {
                        row(
                            service,
                            "execution",
                            execution.execution_arn(),
                            vec![
                                field("name", execution.name()),
                                field("status", execution.status()),
                            ],
                        )
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::StepFunctions, Some("history")) => {
            let response = aws_sdk_sfn::Client::new(config)
                .get_execution_history()
                .execution_arn(&scope.unwrap().id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .events()
                    .iter()
                    .map(|event| {
                        let mut item = row(
                            service,
                            "event",
                            &event.id().to_string(),
                            vec![
                                field("type", event.r#type()),
                                field("timestamp", event.timestamp()),
                            ],
                        );
                        item.details = item.display.clone();
                        item.details.push(field("ID", event.id()));
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::Lambda, None) => {
            let response = aws_sdk_lambda::Client::new(config)
                .list_functions()
                .set_marker(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .functions()
                    .iter()
                    .filter_map(|function| {
                        function.function_arn().map(|arn| {
                            row(
                                service,
                                "function",
                                arn,
                                vec![field("name", function.function_name().unwrap_or(""))],
                            )
                        })
                    })
                    .collect(),
                next_token: response.next_marker().map(str::to_owned),
            })
        }
        (Service::Lambda, Some("mappings")) => {
            let response = aws_sdk_lambda::Client::new(config)
                .list_event_source_mappings()
                .function_name(&scope.unwrap().id)
                .set_marker(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .event_source_mappings()
                    .iter()
                    .map(|mapping| {
                        let mut item = row(
                            service,
                            "mapping",
                            mapping.uuid().unwrap_or("unknown"),
                            vec![
                                field("source ARN", mapping.event_source_arn().unwrap_or("")),
                                field("status", mapping.state().unwrap_or("")),
                            ],
                        );
                        item.details = item.display.clone();
                        if mapping
                            .event_source_arn()
                            .is_some_and(|arn| arn.contains(":sqs:"))
                        {
                            add_link(&mut item.links, mapping.event_source_arn().unwrap_or(""));
                        }
                        item
                    })
                    .collect(),
                next_token: response.next_marker().map(str::to_owned),
            })
        }
        _ => crate::coverage::list_page(config, scope, service, token).await,
    }
}

pub async fn detail(
    config: &SdkConfig,
    _context: &Context,
    resource: Resource,
) -> Result<Resource, String> {
    let mut item = resource;
    match (item.service, item.kind.as_str()) {
        (Service::StepFunctions, "machine") => {
            let response = aws_sdk_sfn::Client::new(config)
                .describe_state_machine()
                .state_machine_arn(&item.id)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("ARN", response.state_machine_arn()),
                field("name", response.name()),
                field("type", response.r#type()),
                field("role", response.role_arn()),
                field("created", response.creation_date()),
            ];
            if let Some(status) = response.status() {
                item.details.push(field("status", status));
            }
            item.details
                .push(field("definition", readable_json(response.definition())));
            item.links = lambda_links(response.definition());
            item.links.insert(
                0,
                ResourceLink {
                    service: Service::StepFunctions,
                    kind: "executions".into(),
                    id: item.id.clone(),
                },
            );
        }
        (Service::StepFunctions, "execution") => {
            let response = aws_sdk_sfn::Client::new(config)
                .describe_execution()
                .execution_arn(&item.id)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("ARN", response.execution_arn()),
                field("machine ARN", response.state_machine_arn()),
                field("status", response.status()),
                field("started", response.start_date()),
            ];
            for (key, value) in [
                ("input", response.input()),
                ("output", response.output()),
                ("error", response.error()),
                ("cause", response.cause()),
            ] {
                if let Some(value) = value {
                    item.details.push(field(key, readable_json(value)));
                }
            }
            if let Some(stopped) = response.stop_date() {
                item.details.push(field("stopped", stopped));
            }
            item.links = vec![ResourceLink {
                service: Service::StepFunctions,
                kind: "history".into(),
                id: item.id.clone(),
            }];
        }
        (Service::Lambda, "function") => {
            let response = aws_sdk_lambda::Client::new(config)
                .get_function_configuration()
                .function_name(&item.id)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("ARN", response.function_arn().unwrap_or(&item.id)),
                field("role", response.role().unwrap_or("")),
            ];
            for (key, value) in [
                ("runtime", response.runtime().map(|value| value.as_str())),
                (
                    "package",
                    response.package_type().map(|value| value.as_str()),
                ),
                ("state", response.state().map(|value| value.as_str())),
                ("last modified", response.last_modified()),
            ] {
                if let Some(value) = value {
                    item.details.push(field(key, value));
                }
            }
            if let Some(timeout) = response.timeout() {
                item.details.push(field("timeout seconds", timeout));
            }
            if let Some(memory) = response.memory_size() {
                item.details.push(field("memory MB", memory));
            }
            if let Some(environment) = response.environment().and_then(|value| value.variables()) {
                let mut names: Vec<_> = environment.keys().cloned().collect();
                names.sort();
                item.details
                    .push(field("environment names", names.join(", ")));
            }
            item.links = vec![ResourceLink {
                service: Service::Lambda,
                kind: "mappings".into(),
                id: item.id.clone(),
            }];
        }
        (Service::StepFunctions, "event") | (Service::Lambda, "mapping") => {}
        _ => return crate::coverage::detail(config, item).await,
    }
    Ok(item)
}

use aws_types::SdkConfig;

use crate::{
    app::Service,
    links::{add_link, integration_link, queue_url, redrive_link},
    model::{Page, Resource, ResourceLink},
};

fn field(name: &str, value: impl ToString) -> (String, String) {
    (name.into(), value.to_string())
}

fn row(
    service: Service,
    kind: &str,
    id: impl Into<String>,
    display: Vec<(String, String)>,
) -> Resource {
    Resource {
        service,
        kind: kind.into(),
        id: id.into(),
        details: display.clone(),
        display,
        links: vec![],
    }
}

fn link(service: Service, kind: &str, id: impl Into<String>) -> ResourceLink {
    ResourceLink {
        service,
        kind: kind.into(),
        id: id.into(),
    }
}

fn pair(first: &str, second: &str) -> String {
    serde_json::to_string(&[first, second]).expect("string pair serializes")
}

fn unpair(id: &str) -> Result<(String, String), String> {
    let parts: Vec<String> = serde_json::from_str(id).map_err(|_| "Invalid resource identifier")?;
    if parts.len() != 2 {
        return Err("Invalid resource identifier".into());
    }
    Ok((parts[0].clone(), parts[1].clone()))
}

fn failure(error: impl std::fmt::Display) -> String {
    let message = error.to_string().to_lowercase();
    if message.contains("accessdenied")
        || message.contains("access denied")
        || message.contains("unauthorized")
    {
        "AWS access denied; check read permissions".into()
    } else if message.contains("throttl") || message.contains("toomanyrequests") {
        "AWS request throttled; retry later".into()
    } else if message.contains("notfound") || message.contains("not found") {
        "AWS resource not found; refresh the source view".into()
    } else {
        "AWS read failed; check network, region and permissions".into()
    }
}

pub fn is_list_kind(kind: &str) -> bool {
    matches!(
        kind,
        "records"
            | "rest_apis"
            | "http_apis"
            | "websocket_apis"
            | "rest_resources"
            | "rest_stages"
            | "http_routes"
            | "http_integrations"
            | "http_stages"
            | "websocket_routes"
            | "websocket_integrations"
            | "websocket_stages"
            | "alarms"
            | "log_groups"
            | "metrics"
            | "log_streams"
            | "log_events"
            | "subscriptions"
            | "rules"
            | "targets"
    )
}

pub fn is_local_detail(kind: &str) -> bool {
    matches!(
        kind,
        "category"
            | "record"
            | "rest_resource"
            | "rest_stage"
            | "v2_route"
            | "v2_integration"
            | "v2_stage"
            | "alarm"
            | "metric"
            | "log_stream"
            | "log_event"
            | "subscription"
            | "target"
    )
}

pub async fn list_page(
    config: &SdkConfig,
    scope: Option<&ResourceLink>,
    service: Service,
    token: Option<&str>,
) -> Result<Page<Resource>, String> {
    let kind = scope.map(|value| value.kind.as_str());
    let id = scope.map(|value| value.id.as_str()).unwrap_or("");
    match (service, kind) {
        (Service::Route53, None) => {
            let response = aws_sdk_route53::Client::new(config)
                .list_hosted_zones()
                .set_marker(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .hosted_zones()
                    .iter()
                    .map(|zone| {
                        let mut item = row(
                            service,
                            "zone",
                            zone.id(),
                            vec![
                                field("name", zone.name()),
                                field("ID", zone.id()),
                                field(
                                    "type",
                                    if zone.config().is_some_and(|config| config.private_zone()) {
                                        "private"
                                    } else {
                                        "public"
                                    },
                                ),
                            ],
                        );
                        item.links.push(link(service, "records", zone.id()));
                        item
                    })
                    .collect(),
                next_token: response.next_marker().map(str::to_owned),
            })
        }
        (Service::Route53, Some("records")) => {
            let cursor: Option<Vec<String>> = token
                .map(|value| serde_json::from_str(value).map_err(|_| "Invalid record cursor"))
                .transpose()?;
            let mut request = aws_sdk_route53::Client::new(config)
                .list_resource_record_sets()
                .hosted_zone_id(id)
                .max_items(100);
            if let Some(parts) = &cursor {
                if parts.len() != 3 {
                    return Err("Invalid record cursor".into());
                }
                request = request
                    .start_record_name(&parts[0])
                    .start_record_type(aws_sdk_route53::types::RrType::from(parts[1].as_str()));
                if !parts[2].is_empty() {
                    request = request.start_record_identifier(&parts[2]);
                }
            }
            let response = request.send().await.map_err(failure)?;
            let next_token = if response.is_truncated() {
                Some(
                    serde_json::to_string(&[
                        response.next_record_name().unwrap_or(""),
                        response
                            .next_record_type()
                            .map(|value| value.as_str())
                            .unwrap_or(""),
                        response.next_record_identifier().unwrap_or(""),
                    ])
                    .expect("cursor serializes"),
                )
            } else {
                None
            };
            Ok(Page {
                items: response
                    .resource_record_sets()
                    .iter()
                    .map(|record| {
                        let mut fields =
                            vec![field("name", record.name()), field("type", record.r#type())];
                        if let Some(ttl) = record.ttl() {
                            fields.push(field("TTL", ttl));
                        }
                        for value in record.resource_records() {
                            fields.push(field("value", value.value()));
                        }
                        if let Some(alias) = record.alias_target() {
                            fields.push(field("alias target", alias.dns_name()));
                            fields.push(field("alias zone", alias.hosted_zone_id()));
                        }
                        row(
                            service,
                            "record",
                            pair(record.name(), record.r#type().as_str()),
                            fields,
                        )
                    })
                    .collect(),
                next_token,
            })
        }
        (Service::ApiGateway, None) => Ok(Page {
            items: [
                ("rest_apis", "REST APIs"),
                ("http_apis", "HTTP APIs"),
                ("websocket_apis", "WebSocket APIs"),
            ]
            .into_iter()
            .map(|(kind, name)| {
                let mut item = row(service, "category", kind, vec![field("family", name)]);
                item.links.push(link(service, kind, ""));
                item
            })
            .collect(),
            next_token: None,
        }),
        (Service::CloudWatch, None) => Ok(Page {
            items: [
                ("alarms", "alarms"),
                ("log_groups", "log groups"),
                ("metrics", "metric metadata"),
            ]
            .into_iter()
            .map(|(kind, name)| {
                let mut item = row(service, "category", kind, vec![field("view", name)]);
                item.links.push(link(service, kind, ""));
                item
            })
            .collect(),
            next_token: None,
        }),
        (Service::CloudWatch, Some("alarms")) => {
            let response = aws_sdk_cloudwatch::Client::new(config)
                .describe_alarms()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            let metric = response.metric_alarms().iter().map(|alarm| {
                row(
                    service,
                    "alarm",
                    alarm.alarm_name().unwrap_or(""),
                    vec![
                        field("name", alarm.alarm_name().unwrap_or("")),
                        field("type", "metric"),
                        field(
                            "state",
                            alarm
                                .state_value()
                                .map(|value| value.as_str())
                                .unwrap_or(""),
                        ),
                        field("ARN", alarm.alarm_arn().unwrap_or("")),
                        field("metric", alarm.metric_name().unwrap_or("")),
                        field("namespace", alarm.namespace().unwrap_or("")),
                        field(
                            "comparison",
                            alarm
                                .comparison_operator()
                                .map(|value| value.as_str())
                                .unwrap_or(""),
                        ),
                        field(
                            "threshold",
                            alarm
                                .threshold()
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                        ),
                    ],
                )
            });
            let composite = response.composite_alarms().iter().map(|alarm| {
                row(
                    service,
                    "alarm",
                    alarm.alarm_name().unwrap_or(""),
                    vec![
                        field("name", alarm.alarm_name().unwrap_or("")),
                        field("type", "composite"),
                        field(
                            "state",
                            alarm
                                .state_value()
                                .map(|value| value.as_str())
                                .unwrap_or(""),
                        ),
                        field("ARN", alarm.alarm_arn().unwrap_or("")),
                        field("rule", alarm.alarm_rule().unwrap_or("")),
                    ],
                )
            });
            Ok(Page {
                items: metric.chain(composite).collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::CloudWatch, Some("metrics")) => {
            let response = aws_sdk_cloudwatch::Client::new(config)
                .list_metrics()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .metrics()
                    .iter()
                    .map(|metric| {
                        let mut fields = vec![
                            field("name", metric.metric_name().unwrap_or("")),
                            field("namespace", metric.namespace().unwrap_or("")),
                        ];
                        for dimension in metric.dimensions() {
                            fields.push(field(
                                dimension.name().unwrap_or("dimension"),
                                dimension.value().unwrap_or(""),
                            ));
                        }
                        row(
                            service,
                            "metric",
                            pair(
                                metric.namespace().unwrap_or(""),
                                metric.metric_name().unwrap_or(""),
                            ),
                            fields,
                        )
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::CloudWatch, Some("log_groups")) => {
            let response = aws_sdk_cloudwatchlogs::Client::new(config)
                .describe_log_groups()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .log_groups()
                    .iter()
                    .map(|group| {
                        let name = group.log_group_name().unwrap_or("");
                        let mut item = row(
                            service,
                            "log_group",
                            name,
                            vec![
                                field("name", name),
                                field("ARN", group.arn().unwrap_or("")),
                                field("stored bytes", group.stored_bytes().unwrap_or(0)),
                            ],
                        );
                        if let Some(days) = group.retention_in_days() {
                            item.details.push(field("retention days", days));
                        }
                        item.links.push(link(service, "log_streams", name));
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::CloudWatch, Some("log_streams")) => {
            let response = aws_sdk_cloudwatchlogs::Client::new(config)
                .describe_log_streams()
                .log_group_name(id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .log_streams()
                    .iter()
                    .map(|stream| {
                        let name = stream.log_stream_name().unwrap_or("");
                        let mut item = row(
                            service,
                            "log_stream",
                            pair(id, name),
                            vec![
                                field("name", name),
                                field("ARN", stream.arn().unwrap_or("")),
                            ],
                        );
                        item.links.push(link(service, "log_events", pair(id, name)));
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::CloudWatch, Some("log_events")) => {
            let (group, stream) = unpair(id)?;
            let response = aws_sdk_cloudwatchlogs::Client::new(config)
                .get_log_events()
                .log_group_name(group)
                .log_stream_name(stream)
                .start_from_head(true)
                .limit(100)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            let next = response
                .next_forward_token()
                .filter(|next| Some(*next) != token)
                .map(str::to_owned);
            Ok(Page {
                items: response
                    .events()
                    .iter()
                    .enumerate()
                    .map(|(index, event)| {
                        row(
                            service,
                            "log_event",
                            format!("{}:{index}", event.timestamp().unwrap_or(0)),
                            vec![
                                field("timestamp", event.timestamp().unwrap_or(0)),
                                field("message", event.message().unwrap_or("")),
                            ],
                        )
                    })
                    .collect(),
                next_token: next,
            })
        }
        (Service::Sns, None) => {
            let response = aws_sdk_sns::Client::new(config)
                .list_topics()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .topics()
                    .iter()
                    .filter_map(|topic| {
                        topic.topic_arn().map(|arn| {
                            let mut item = row(
                                service,
                                "topic",
                                arn,
                                vec![
                                    field("name", arn.rsplit(':').next().unwrap_or(arn)),
                                    field("ARN", arn),
                                ],
                            );
                            item.links.push(link(service, "subscriptions", arn));
                            item
                        })
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::Sns, Some("subscriptions")) => {
            let response = aws_sdk_sns::Client::new(config)
                .list_subscriptions_by_topic()
                .topic_arn(id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .subscriptions()
                    .iter()
                    .map(|subscription| {
                        let mut item = row(
                            service,
                            "subscription",
                            subscription
                                .subscription_arn()
                                .unwrap_or("PendingConfirmation"),
                            vec![
                                field("protocol", subscription.protocol().unwrap_or("")),
                                field("endpoint", subscription.endpoint().unwrap_or("")),
                                field("ARN", subscription.subscription_arn().unwrap_or("")),
                            ],
                        );
                        if matches!(subscription.protocol(), Some("lambda" | "sqs")) {
                            add_link(&mut item.links, subscription.endpoint().unwrap_or(""));
                        }
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::Sqs, None) => {
            let response = aws_sdk_sqs::Client::new(config)
                .list_queues()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .queue_urls()
                    .iter()
                    .map(|url| {
                        row(
                            service,
                            "queue",
                            url,
                            vec![
                                field("name", url.rsplit('/').next().unwrap_or(url)),
                                field("URL", url),
                            ],
                        )
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::EventBridge, None) => {
            let response = aws_sdk_eventbridge::Client::new(config)
                .list_event_buses()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .event_buses()
                    .iter()
                    .map(|bus| {
                        let name = bus.name().unwrap_or("");
                        let mut item = row(
                            service,
                            "bus",
                            name,
                            vec![field("name", name), field("ARN", bus.arn().unwrap_or(""))],
                        );
                        item.links.push(link(service, "rules", name));
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::EventBridge, Some("rules")) => {
            let response = aws_sdk_eventbridge::Client::new(config)
                .list_rules()
                .event_bus_name(id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .rules()
                    .iter()
                    .map(|rule| {
                        let name = rule.name().unwrap_or("");
                        let mut item = row(
                            service,
                            "rule",
                            pair(id, name),
                            vec![
                                field("name", name),
                                field("ARN", rule.arn().unwrap_or("")),
                                field(
                                    "state",
                                    rule.state().map(|state| state.as_str()).unwrap_or(""),
                                ),
                            ],
                        );
                        item.links.push(link(service, "targets", pair(id, name)));
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::EventBridge, Some("targets")) => {
            let (bus, rule) = unpair(id)?;
            let response = aws_sdk_eventbridge::Client::new(config)
                .list_targets_by_rule()
                .event_bus_name(bus)
                .rule(rule)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .targets()
                    .iter()
                    .map(|target| {
                        let mut item = row(
                            service,
                            "target",
                            target.id(),
                            vec![field("ID", target.id()), field("ARN", target.arn())],
                        );
                        add_link(&mut item.links, target.arn());
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        (Service::ApiGateway, _) => api_list_page(config, scope, token).await,
        _ => Err("No read adapter for selected view".into()),
    }
}

pub async fn detail(config: &SdkConfig, mut item: Resource) -> Result<Resource, String> {
    match (item.service, item.kind.as_str()) {
        (Service::Route53, "zone") => {
            let response = aws_sdk_route53::Client::new(config)
                .get_hosted_zone()
                .id(&item.id)
                .send()
                .await
                .map_err(failure)?;
            let zone = response
                .hosted_zone()
                .ok_or("Hosted zone missing from AWS response")?;
            item.details = vec![
                field("name", zone.name()),
                field("ID", zone.id()),
                field(
                    "record count",
                    zone.resource_record_set_count().unwrap_or(0),
                ),
            ];
            if let Some(config) = zone.config() {
                item.details.push(field("private", config.private_zone()));
                if let Some(comment) = config.comment() {
                    item.details.push(field("comment", comment));
                }
            }
        }
        (Service::Sns, "topic") => {
            let response = aws_sdk_sns::Client::new(config)
                .get_topic_attributes()
                .topic_arn(&item.id)
                .send()
                .await
                .map_err(failure)?;
            item.details = response
                .attributes()
                .into_iter()
                .flat_map(|attrs| attrs.iter())
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "TopicArn"
                            | "DisplayName"
                            | "SubscriptionsConfirmed"
                            | "SubscriptionsPending"
                            | "SubscriptionsDeleted"
                            | "FifoTopic"
                            | "ContentBasedDeduplication"
                    )
                })
                .map(|(key, value)| field(key, value))
                .collect();
        }
        (Service::Sqs, "queue") => {
            let url = queue_url(&item.id).unwrap_or_else(|| item.id.clone());
            let response = aws_sdk_sqs::Client::new(config)
                .get_queue_attributes()
                .queue_url(&url)
                .attribute_names(aws_sdk_sqs::types::QueueAttributeName::All)
                .send()
                .await
                .map_err(failure)?;
            item.details = response
                .attributes()
                .into_iter()
                .flat_map(|attrs| attrs.iter())
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "QueueArn"
                            | "ApproximateNumberOfMessages"
                            | "ApproximateNumberOfMessagesDelayed"
                            | "ApproximateNumberOfMessagesNotVisible"
                            | "MessageRetentionPeriod"
                            | "FifoQueue"
                    )
                })
                .map(|(key, value)| field(key.as_str(), value))
                .collect();
            if let Some(policy) = response
                .attributes()
                .and_then(|attrs| attrs.get(&aws_sdk_sqs::types::QueueAttributeName::RedrivePolicy))
            {
                item.details.push(field("redrive policy", policy));
                if let Some(target) = redrive_link(policy)
                    && !item.links.contains(&target)
                {
                    item.links.push(target);
                }
            }
        }
        (Service::EventBridge, "bus") => {
            let response = aws_sdk_eventbridge::Client::new(config)
                .describe_event_bus()
                .name(&item.id)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("name", response.name().unwrap_or("")),
                field("ARN", response.arn().unwrap_or("")),
            ];
            if let Some(description) = response.description() {
                item.details.push(field("description", description));
            }
        }
        (Service::EventBridge, "rule") => {
            let (bus, rule) = unpair(&item.id)?;
            let response = aws_sdk_eventbridge::Client::new(config)
                .describe_rule()
                .event_bus_name(bus)
                .name(rule)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("name", response.name().unwrap_or("")),
                field("ARN", response.arn().unwrap_or("")),
                field(
                    "state",
                    response.state().map(|value| value.as_str()).unwrap_or(""),
                ),
            ];
            for (key, value) in [
                ("schedule", response.schedule_expression()),
                ("pattern", response.event_pattern()),
                ("description", response.description()),
            ] {
                if let Some(value) = value {
                    item.details.push(field(key, value));
                }
            }
        }
        (Service::ApiGateway, _) => return api_detail(config, item).await,
        _ if is_local_detail(&item.kind) || item.kind == "log_group" => {}
        _ => return Err("No detail adapter for selected resource".into()),
    }
    Ok(item)
}

async fn api_list_page(
    config: &SdkConfig,
    scope: Option<&ResourceLink>,
    token: Option<&str>,
) -> Result<Page<Resource>, String> {
    let scope = scope.ok_or("Select an API family first")?;
    let service = Service::ApiGateway;
    match scope.kind.as_str() {
        "rest_apis" => {
            let response = aws_sdk_apigateway::Client::new(config)
                .get_rest_apis()
                .set_position(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .items()
                    .iter()
                    .filter_map(|api| {
                        api.id().map(|id| {
                            let mut item = row(
                                service,
                                "rest_api",
                                id,
                                vec![
                                    field("name", api.name().unwrap_or("")),
                                    field("ID", id),
                                    field("type", "REST"),
                                ],
                            );
                            item.links.extend([
                                link(service, "rest_resources", id),
                                link(service, "rest_stages", id),
                            ]);
                            item
                        })
                    })
                    .collect(),
                next_token: response.position().map(str::to_owned),
            })
        }
        "http_apis" | "websocket_apis" => {
            let response = aws_sdk_apigatewayv2::Client::new(config)
                .get_apis()
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            let family = if scope.kind == "http_apis" {
                "HTTP"
            } else {
                "WEBSOCKET"
            };
            Ok(Page {
                items: response
                    .items()
                    .iter()
                    .filter(|api| {
                        api.protocol_type()
                            .is_some_and(|protocol| protocol.as_str() == family)
                    })
                    .filter_map(|api| {
                        api.api_id().map(|id| {
                            let mut item = row(
                                service,
                                "v2_api",
                                pair(family, id),
                                vec![
                                    field("name", api.name().unwrap_or("")),
                                    field("ID", id),
                                    field("type", family),
                                ],
                            );
                            let prefix = if family == "HTTP" {
                                "http"
                            } else {
                                "websocket"
                            };
                            for suffix in ["routes", "integrations", "stages"] {
                                item.links.push(link(
                                    service,
                                    &format!("{prefix}_{suffix}"),
                                    pair(family, id),
                                ));
                            }
                            item
                        })
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        "rest_resources" => {
            let response = aws_sdk_apigateway::Client::new(config)
                .get_resources()
                .rest_api_id(&scope.id)
                .embed("methods")
                .set_position(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .items()
                    .iter()
                    .filter_map(|resource| {
                        resource.id().map(|id| {
                            let path = resource.path().unwrap_or("");
                            let mut item = row(
                                service,
                                "rest_resource",
                                pair(&scope.id, id),
                                vec![field("path", path), field("ID", id)],
                            );
                            if let Some(methods) = resource.resource_methods() {
                                let mut names: Vec<_> = methods.keys().collect();
                                names.sort();
                                for method in names {
                                    item.links.push(link(
                                        service,
                                        "rest_method",
                                        pair(&scope.id, &pair(id, method)),
                                    ));
                                }
                            }
                            item
                        })
                    })
                    .collect(),
                next_token: response.position().map(str::to_owned),
            })
        }
        "rest_stages" => {
            let response = aws_sdk_apigateway::Client::new(config)
                .get_stages()
                .rest_api_id(&scope.id)
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .item()
                    .iter()
                    .map(|stage| {
                        row(
                            service,
                            "rest_stage",
                            pair(&scope.id, stage.stage_name().unwrap_or("")),
                            vec![
                                field("stage", stage.stage_name().unwrap_or("")),
                                field("deployment", stage.deployment_id().unwrap_or("")),
                                field("description", stage.description().unwrap_or("")),
                            ],
                        )
                    })
                    .collect(),
                next_token: None,
            })
        }
        "http_routes" | "websocket_routes" => {
            let (family, id) = unpair(&scope.id)?;
            let response = aws_sdk_apigatewayv2::Client::new(config)
                .get_routes()
                .api_id(id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .items()
                    .iter()
                    .map(|route| {
                        row(
                            service,
                            "v2_route",
                            pair(&scope.id, route.route_id().unwrap_or("")),
                            vec![
                                field("route", route.route_key().unwrap_or("")),
                                field("ID", route.route_id().unwrap_or("")),
                                field("type", &family),
                                field("target", route.target().unwrap_or("")),
                                field(
                                    "authorization",
                                    route
                                        .authorization_type()
                                        .map(|value| value.as_str())
                                        .unwrap_or(""),
                                ),
                            ],
                        )
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        "http_integrations" | "websocket_integrations" => {
            let (family, id) = unpair(&scope.id)?;
            let response = aws_sdk_apigatewayv2::Client::new(config)
                .get_integrations()
                .api_id(id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .items()
                    .iter()
                    .map(|integration| {
                        let mut item = row(
                            service,
                            "v2_integration",
                            pair(&scope.id, integration.integration_id().unwrap_or("")),
                            vec![
                                field("ID", integration.integration_id().unwrap_or("")),
                                field(
                                    "type",
                                    integration
                                        .integration_type()
                                        .map(|value| value.as_str())
                                        .unwrap_or(""),
                                ),
                                field("API type", &family),
                                field("URI", integration.integration_uri().unwrap_or("")),
                            ],
                        );
                        if let Some(target) =
                            integration_link(integration.integration_uri().unwrap_or(""))
                        {
                            item.links.push(target);
                        }
                        item
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        "http_stages" | "websocket_stages" => {
            let (family, id) = unpair(&scope.id)?;
            let response = aws_sdk_apigatewayv2::Client::new(config)
                .get_stages()
                .api_id(id)
                .set_next_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(failure)?;
            Ok(Page {
                items: response
                    .items()
                    .iter()
                    .map(|stage| {
                        row(
                            service,
                            "v2_stage",
                            pair(&scope.id, stage.stage_name().unwrap_or("")),
                            vec![
                                field("stage", stage.stage_name().unwrap_or("")),
                                field("type", &family),
                                field("deployment", stage.deployment_id().unwrap_or("")),
                                field("auto deploy", stage.auto_deploy().unwrap_or(false)),
                            ],
                        )
                    })
                    .collect(),
                next_token: response.next_token().map(str::to_owned),
            })
        }
        _ => Err("No API Gateway read adapter for selected view".into()),
    }
}

async fn api_detail(config: &SdkConfig, mut item: Resource) -> Result<Resource, String> {
    match item.kind.as_str() {
        "rest_api" => {
            let response = aws_sdk_apigateway::Client::new(config)
                .get_rest_api()
                .rest_api_id(&item.id)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("name", response.name().unwrap_or("")),
                field("ID", response.id().unwrap_or("")),
                field("description", response.description().unwrap_or("")),
            ];
        }
        "v2_api" => {
            let (family, id) = unpair(&item.id)?;
            let response = aws_sdk_apigatewayv2::Client::new(config)
                .get_api()
                .api_id(id)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("name", response.name().unwrap_or("")),
                field("type", family),
                field("endpoint", response.api_endpoint().unwrap_or("")),
                field("description", response.description().unwrap_or("")),
            ];
        }
        "rest_method" => {
            let (api, remainder) = unpair(&item.id)?;
            let (resource, method) = unpair(&remainder)?;
            let client = aws_sdk_apigateway::Client::new(config);
            let response = client
                .get_method()
                .rest_api_id(&api)
                .resource_id(&resource)
                .http_method(&method)
                .send()
                .await
                .map_err(failure)?;
            item.details = vec![
                field("method", &method),
                field("authorization", response.authorization_type().unwrap_or("")),
                field("API", &api),
                field("resource", &resource),
            ];
            if response.method_integration().is_some() {
                let integration = client
                    .get_integration()
                    .rest_api_id(&api)
                    .resource_id(&resource)
                    .http_method(&method)
                    .send()
                    .await
                    .map_err(failure)?;
                item.details.push(field(
                    "integration type",
                    integration
                        .r#type()
                        .map(|value| value.as_str())
                        .unwrap_or(""),
                ));
                if let Some(uri) = integration.uri() {
                    item.details.push(field("integration URI", uri));
                    if let Some(target) = integration_link(uri) {
                        item.links.push(target);
                    }
                }
            }
        }
        _ if is_local_detail(&item.kind) => {}
        _ => return Err("No API Gateway detail adapter for selected resource".into()),
    }
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{App, Context, LoadState, View},
        model::{ServiceReadAdapter, read_pages},
    };

    #[test]
    fn scoped_identifiers_preserve_parent_and_reject_bad_input() {
        let id = pair("bus/with \"quotes\"", "rule\nname");
        assert_eq!(
            unpair(&id).unwrap(),
            ("bus/with \"quotes\"".into(), "rule\nname".into())
        );
        assert!(unpair("not JSON").is_err());
        assert!(unpair("[\"only one\"]").is_err());
        assert!(is_list_kind("targets"));
        assert!(!is_list_kind("rule"));
    }

    #[test]
    fn errors_do_not_expose_sdk_response_bodies() {
        assert_eq!(
            failure("AccessDenied: secret credential"),
            "AWS access denied; check read permissions"
        );
        assert_eq!(
            failure("unrecognized body with private data"),
            "AWS read failed; check network, region and permissions"
        );
    }

    struct Fixture {
        service: Service,
        denied: bool,
        empty: bool,
    }

    impl ServiceReadAdapter for Fixture {
        type Item = Resource;

        async fn list_page(&self, token: Option<&str>) -> Result<Page<Resource>, String> {
            if self.denied && token.is_some() {
                return Err(failure("AccessDenied"));
            }
            if self.empty {
                return Ok(Page {
                    items: vec![],
                    next_token: None,
                });
            }
            let id = if token.is_some() { "second" } else { "first" };
            Ok(Page {
                items: vec![row(self.service, "fixture", id, vec![field("name", id)])],
                next_token: token.is_none().then(|| "next".into()),
            })
        }
    }

    #[tokio::test]
    async fn every_service_family_handles_pages_empty_denied_and_stale_context() {
        for service in [
            Service::Route53,
            Service::ApiGateway,
            Service::CloudWatch,
            Service::Sns,
            Service::Sqs,
            Service::EventBridge,
        ] {
            let mut app = App::new(Context {
                account: Some("123".into()),
                profile: None,
                region: "us-east-1".into(),
            });
            app.select(service);
            let generation = app.view_generation;
            read_pages(
                &Fixture {
                    service,
                    denied: false,
                    empty: false,
                },
                |items, done| app.apply_page(0, generation, items, done),
            )
            .await
            .unwrap();
            assert!(
                matches!(&app.view, View::List(view) if view.rows.len() == 2 && view.load == LoadState::Complete),
                "{service:?}"
            );
            app.select(service);
            let generation = app.view_generation;
            read_pages(
                &Fixture {
                    service,
                    denied: false,
                    empty: true,
                },
                |items, done| app.apply_page(0, generation, items, done),
            )
            .await
            .unwrap();
            assert!(
                matches!(&app.view, View::List(view) if view.rows.is_empty() && view.load == LoadState::Complete),
                "{service:?}"
            );
            app.select(service);
            let generation = app.view_generation;
            let error = read_pages(
                &Fixture {
                    service,
                    denied: true,
                    empty: false,
                },
                |items, done| app.apply_page(0, generation, items, done),
            )
            .await
            .unwrap_err();
            app.apply_error(0, generation, error);
            assert!(
                matches!(&app.view, View::List(view) if view.rows.len() == 1 && matches!(view.load, LoadState::Failed(_))),
                "{service:?}"
            );
            app.replace_context(Context {
                account: Some("456".into()),
                profile: None,
                region: "eu-west-1".into(),
            });
            app.select(service);
            app.apply_page(
                0,
                generation,
                vec![row(service, "fixture", "stale", vec![])],
                true,
            );
            assert!(
                matches!(&app.view, View::List(view) if view.rows.is_empty()),
                "{service:?}"
            );
        }
    }
}

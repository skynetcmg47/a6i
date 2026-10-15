# Milestone 2: Remaining service coverage

Status: complete (2026-10-15). Depends on: milestone 1. Delivers browse/detail for every requested service; cross-links beyond Step Functions-to-Lambda wait for milestone 3.

## Service matrix

| Service | List and child views | Detail and allowed read APIs |
| --- | --- | --- |
| Route 53 | Hosted zones, then paginated record sets | `ListHostedZones`, `GetHostedZone`, `ListResourceRecordSets`. Show zone ID, type, record name/type/value or alias target. Never change records. |
| API Gateway | REST APIs with resources/methods/integrations/stages; HTTP and WebSocket APIs with routes/integrations/stages | API Gateway v1 `GetRestApis`, `GetRestApi`, `GetResources`, `GetMethod`, `GetIntegration`, `GetStages`; v2 `GetApis`, `GetApi`, `GetRoutes`, `GetIntegrations`, `GetStages`. Display route, integration URI, stage metadata. Fetch method/integration details on selection, not a full API crawl. |
| CloudWatch | Alarms; log groups, log streams, log events; metric names/metadata | CloudWatch `DescribeAlarms`, `ListMetrics`; CloudWatch Logs `DescribeLogGroups`, `DescribeLogStreams`, `GetLogEvents`. Never fetch metric datapoints or graph them. Show log events in bounded, scrollable pages without local persistence. |
| SNS | Topics and topic subscriptions | `ListTopics`, `GetTopicAttributes`, `ListSubscriptionsByTopic`. Show subscription protocol and endpoint; no publish. |
| SQS | Queues and queue attributes | `ListQueues`, `GetQueueAttributes`. Show ARN, counts, retention, and dead-letter target from redrive policy when present. Never `ReceiveMessage`, `DeleteMessage`, or `PurgeQueue`. |
| EventBridge | Event buses, rules per bus, targets per rule | `ListEventBuses`, `DescribeEventBus`, `ListRules`, `DescribeRule`, `ListTargetsByRule`. Preserve bus name when opening rule/target. No event publishing. |

Use service-specific SDK paginators or next-token loops. For APIs without the same pagination shape, normalize at the adapter boundary. All children belong to the selected parent and active account/region; changing context clears them. Fetch child lists only on opening their parent. API Gateway's REST and v2 API families remain distinguishable in one service selector.

## Presentation

- Lists show resource name, ID/ARN when available, type, and useful status. Details show returned fields without exposing credentials or secret values; avoid dumping raw SDK debug output.
- Render long JSON, records, and log lines safely within terminal bounds. Escape control characters in AWS-provided strings so resource content cannot manipulate terminal state. Mark empty lists and access-denied child views explicitly.
- Keep `/` local to current list; paginate remaining pages in background. Do not traverse every API method, CloudWatch stream, record set, or rule until user opens its parent.

## Acceptance

- Offline fixtures cover one happy path, empty result, next-page result, access-denied error, and canceled/stale request for each service family.
- API Gateway REST/HTTP/WebSocket routes and integrations are not mixed; Route 53 works with selected regional context without treating zones as region-scoped.
- CloudWatch log events and Lambda environment values never enter application logs or persistent files. Every SDK call is present in reviewed read-only inventory.

# a6i implementation plan

Status: milestones 0, 1, and 2 complete. Work through milestones in order; update milestone status only after implementation and verification.

## Goal

Build a read-only, k9s-like AWS terminal browser named `a6i`. Users can list resources, inspect details, filter the current list, and follow references to other supported resources. Initial services: Route 53, API Gateway, Lambda, Step Functions, CloudWatch, SNS, SQS, and EventBridge (the requested "eventbright").

Use Rust: installed toolchain, Ratatui + Crossterm for TUI, Tokio for background work, official AWS SDK for Rust. Keep dependencies and modules minimal. No AWS CLI subprocesses, arbitrary AWS operation dispatcher, write APIs, resource mutation, or local persistence of AWS response data. Read requests to AWS are allowed; the app does not operate offline.

## Milestones

| Order | File | Deliverable | Status |
| --- | --- | --- | --- |
| 0 | [00-foundation.md](00-foundation.md) | Safe TUI, credentials, region/profile switching, navigation and search | Complete |
| 1 | [01-first-navigation.md](01-first-navigation.md) | Step Functions and Lambda browsing, first cross-resource link | Complete |
| 2 | [02-service-coverage.md](02-service-coverage.md) | Remaining six services and their natural child resources | Complete |
| 3 | [03-links-and-release.md](03-links-and-release.md) | Supported cross-links, safety audit, documentation, release checks | Planned |

## Shared behavior

- `a6i [--profile NAME] [--region REGION]` uses the SDK default credential/region chain when flags are absent. A missing region is an actionable error for regional services; Route 53 is global.
- Show active account ID (via STS `GetCallerIdentity`), profile label, region, service, list loading/completion state, and errors. If identity lookup fails, do not claim an account ID or follow account-sensitive links.
- `:route53`, `:apigateway`, `:lambda`, `:stepfunctions`, `:cloudwatch`, `:sns`, `:sqs`, `:eventbridge` select services. `:profile NAME` and `:region REGION` change context. Unknown commands fail locally without any AWS call. No shell escape or free-form AWS command input.
- `/` starts case-insensitive substring filtering of the current list's loaded items; `Enter` opens a row or selected link, `Esc` goes back, `r` refreshes, `?` opens help, `q` quits. A detail view presents links as selectable rows; breadcrumbs preserve prior list/filter/selection. Typing in search or command mode must not trigger normal key bindings.
- Fetch every page for the active list in background, sequentially with SDK pagination tokens. Mark results partial until exhausted; filter never claims account-wide coverage. Cancel requests on view/profile/region change and discard any late responses from the old context. Refresh replaces the active list, not unrelated views.
- Display access-denied, missing-resource, throttling, and network errors without terminating the TUI. Preserve already loaded rows and mark lists partial on page failure. Detail errors keep the previous view navigable. Do not log credentials, resource contents, or log events.
- Read-only means a compile-time, explicit inventory of SDK read operations, not IAM action-name guessing. AWS credentials should also be read-only, but app safety cannot rely on IAM alone. Read calls can incur AWS charges; do not auto-scan other regions or services.

## Scope boundaries

- One active region at a time; Route 53 zones and records use its global endpoint. No multi-region aggregation or account-wide search.
- No SQS message receiving (it changes visibility), Lambda invocation or code download, Step Functions start/stop, SNS publish, EventBridge put events, Route 53 changes, API Gateway deployment, metric graphs, or persistent snapshots.
- Link only from explicit, static references returned by AWS. Never infer a Lambda from a state name or guess an ARN from display text. Unsupported or dynamic references remain visible as text.
- No new service beyond the eight above plus STS identity lookup. No AWS console/browser launch is required.

## Execution convention

Implement one milestone at a time. Add a small runnable offline check for each nontrivial parser or state transition. Use fake read clients or fixtures; never require live AWS credentials for automated tests. Keep README user instructions current as milestones land. Do not mark a milestone complete until its acceptance checks pass.

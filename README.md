# a6i

Read-only AWS terminal browser. Browse Route 53, API Gateway, Lambda, Step Functions, CloudWatch, SNS, SQS, and EventBridge in one selected region (Route 53 is global).

## Install

For Linux x86_64, download `a6i-linux-x86_64` and `a6i-linux-x86_64.sha256` from a [GitHub Release](https://github.com/skynetcmg47/a6i/releases). Verify both files in the same directory with `sha256sum -c a6i-linux-x86_64.sha256`, then run `chmod +x a6i-linux-x86_64` and `./a6i-linux-x86_64 --help`. Releases use `vMAJOR.MINOR.PATCH` tags on successful pushes to `main`; the patch number comes from the workflow run number plus the Cargo patch version, not a Cargo version change.

Alternatively, install a Rust toolchain and build from source with `cargo build --release`. Run `target/release/a6i --help` to check the binary. No AWS credentials are needed for help.

## Run

Requires an interactive terminal. Configure AWS credentials using a named profile or the AWS SDK credential chain, then run the downloaded binary or build from source:

```sh
cargo run -- --profile my-profile --region us-east-1
```

For the Linux release binary, replace `cargo run -- --` with `./a6i-linux-x86_64`. Building from source requires Rust.

Both flags are optional when the SDK chain provides them. `cargo run -- --help` works without credentials or a terminal. Missing region or credentials produce an error; failed STS identity lookup shows `unavailable` instead of an account ID. A named profile must exist in AWS config files; `:profile NAME` and `:region REGION` switch context after validation. AWS read calls can incur charges; credential providers may perform authentication requests (such as role assumption or SSO) as part of credential resolution.

## Keys

`Enter` selects a resource or link; `Esc` returns without exiting; `:` starts a command; `/` filters loaded rows locally; `r` refreshes the active view; `PgUp`/`PgDn` scroll detail text; `j`/`k` select links; `n` fetches the next page of CloudWatch log events (100 per request); `?` shows help. Type `:exit` or `:quit` and press `Enter` to exit. Commands: `:route53`, `:apigateway`, `:lambda`, `:stepfunctions`, `:cloudwatch`, `:sns`, `:sqs`, `:eventbridge`, `:profile NAME`, `:region REGION`. API Gateway separates REST, HTTP, and WebSocket APIs. CloudWatch separates alarms, log groups, and metric metadata. Child lists load only on opening a parent. Invalid commands never reach AWS.

Help opens in the main pane; `Esc` or `?` closes it. List previews point to `Enter` for scrollable full details. Filtered lists show matching/loaded counts and distinguish no matches from an empty service. Footer hints change with the current view; `n` appears only when another log-event page is available.

Static links cover Step Functions tasks to Lambda, EventBridge targets to Lambda/Step Functions/SNS/SQS, API Gateway integrations to literal Lambda ARNs, SNS subscription endpoints to Lambda/SQS, SQS dead-letter queues, and Lambda mappings to SQS. Dynamic, malformed, external, or unsupported references remain text. Links load targets only when selected, with no cross-service search. Same-account cross-region links require `Enter` confirmation; `Esc` cancels or restores the source view and region. Cross-account links do not open: switch to a suitable profile explicitly. If STS identity is unavailable, account-sensitive links stay disabled. Nested cross-region jumps require returning to the source first.

Application read-operation inventory:

- STS: `GetCallerIdentity`.
- Step Functions: `ListStateMachines`, `DescribeStateMachine`, `ListExecutions`, `DescribeExecution`, `GetExecutionHistory`.
- Lambda: `ListFunctions`, `GetFunctionConfiguration`, `ListEventSourceMappings`.
- Route 53: `ListHostedZones`, `GetHostedZone`, `ListResourceRecordSets` (global endpoint, independent of selected region).
- API Gateway REST: `GetRestApis`, `GetRestApi`, `GetResources`, `GetMethod`, `GetIntegration`, `GetStages`.
- API Gateway HTTP/WebSocket: `GetApis`, `GetApi`, `GetRoutes`, `GetIntegrations`, `GetStages`.
- CloudWatch: `DescribeAlarms`, `ListMetrics`; CloudWatch Logs: `DescribeLogGroups`, `DescribeLogStreams`, `GetLogEvents`.
- SNS: `ListTopics`, `GetTopicAttributes`, `ListSubscriptionsByTopic`.
- SQS: `ListQueues`, `GetQueueAttributes` (no message receives).
- EventBridge: `ListEventBuses`, `DescribeEventBus`, `ListRules`, `DescribeRule`, `ListTargetsByRule`.

No generic operation execution, shell escape, AWS CLI subprocess, or resource mutation. The offline inventory test fails if a new SDK send call lacks an entry above. Profile and region switches replace context only after credentials resolve; pending reads cancel and late responses are ignored. Lists show partial/error states when pagination stops early. Log event pages stay in memory for the current view only; no metric datapoints are fetched. Lambda environment variable names appear, never values. State machine definitions, execution input/output, and history display only in terminal; no AWS response data is written to disk.

## Test

Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release`. Then run `python3 tests/smoke_mock.py` to exercise all eight service families, detail/child views, cross-service links, cross-region confirmation, and bounded log pagination against a local fake AWS endpoint. The smoke test needs a terminal-capable host and uses dummy credentials; it does not contact AWS. Live resource paths additionally require an account with representative resources and read permissions.

## IAM safety

Grant only needed read actions for services you browse, scoped to resources where AWS supports resource-level permissions. Suggested actions from inventory (IAM names differ from SDK method names for API Gateway):

- Step Functions: `states:ListStateMachines`, `states:DescribeStateMachine`, `states:ListExecutions`, `states:DescribeExecution`, `states:GetExecutionHistory`.
- Lambda: `lambda:ListFunctions`, `lambda:GetFunctionConfiguration`, `lambda:ListEventSourceMappings`.
- Route 53: `route53:ListHostedZones`, `route53:GetHostedZone`, `route53:ListResourceRecordSets`.
- API Gateway REST and v2 control plane: `apigateway:GET` on relevant management API paths.
- CloudWatch: `cloudwatch:DescribeAlarms`, `cloudwatch:ListMetrics`; Logs: `logs:DescribeLogGroups`, `logs:DescribeLogStreams`, `logs:GetLogEvents`.
- SNS: `sns:ListTopics`, `sns:GetTopicAttributes`, `sns:ListSubscriptionsByTopic`; SQS: `sqs:ListQueues`, `sqs:GetQueueAttributes`.
- EventBridge: `events:ListEventBuses`, `events:DescribeEventBus`, `events:ListRules`, `events:DescribeRule`, `events:ListTargetsByRule`.

STS `GetCallerIdentity` does not require an IAM allow action; credential sources such as SSO or role assumption may need separate authentication permissions. Application read-only controls and restrictive IAM are separate safety layers; neither replaces the other. Avoid broad policies or write actions.

## Develop

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
cargo build --release
```

Tests use no live AWS credentials.

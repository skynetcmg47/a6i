# Milestone 1: Step Functions and Lambda

Status: complete (2026-10-15). Depends on: milestone 0. Delivers first useful end-to-end browse and link flow.

## Step Functions

- List state machines with pagination (`ListStateMachines`). Open machine detail (`DescribeStateMachine`) with ARN, type/status, role, creation/update data available from API, and definition in a scrollable read-only pane.
- From a machine, list executions (`ListExecutions`); open execution detail (`DescribeExecution`) and paginated event history (`GetExecutionHistory`). Show input/output as read-only content, with truncation or scrolling rather than terminal overflow. Treat potentially sensitive fields as screen-only data; never persist or log them.
- Parse the returned Amazon States Language JSON, recursively visiting `States` under top-level, `Parallel` branches, and `Map` item processors/iterators. Recognize direct Lambda ARN task resources and optimized `arn:aws:states:::lambda:invoke` tasks with literal Lambda ARN in `Parameters.FunctionName` or `Arguments.FunctionName`. Do not resolve JSONPath, JSONata expressions, variables, aliases through extra calls, or runtime-generated names. Unsupported references remain plain text.

## Lambda

- Paginate functions (`ListFunctions`) and show configuration detail (`GetFunctionConfiguration`): ARN, runtime/package type, state, role, timeout, memory, environment variable **names only**, and other non-secret metadata. Never display environment values.
- List event-source mappings for selected function (`ListEventSourceMappings`) with source ARN and status. Do not call invoke, update, or code-download APIs.
- Selecting a static Step Functions Lambda link opens that function in current profile/region if account and region match. On missing function or access denied, retain source detail and show error. Other-region handling is completed in milestone 3.

## Acceptance

- State machine, execution, history, Lambda, and event-source lists handle multiple pages, empty pages, API errors, and cancellation.
- Fixture tests cover nested ASL, direct/optimized Lambda tasks, missing/dynamic `FunctionName`, malformed JSON, duplicate references, and wrong-account/region references.
- A static machine-to-Lambda link opens detail and `Esc` restores machine view with prior filter/selection. No test uses live AWS credentials.

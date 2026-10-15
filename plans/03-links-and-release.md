# Milestone 3: Cross-links, safety audit, release

Status: implemented (2026-10-15). Depends on: milestone 2. Delivers complete v1 navigation and release checks. Optional live AWS smoke check remains.

## Link rules

- Keep Step Functions-to-Lambda links from milestone 1. Add EventBridge target ARN links to supported Lambda, Step Functions, SNS, and SQS resources; API Gateway integration links only when a static integration URI unambiguously identifies a supported target; SNS subscription endpoint links for Lambda/SQS ARNs; SQS dead-letter queue links from redrive policy; Lambda event-source mapping links for supported SQS source ARNs.
- Parse ARNs structurally (partition, service, region, account, resource). Link only recognized static references. Dedupe equivalent links; show unresolved, dynamic, external, and unsupported targets as plain text. Do not infer links from name similarity, DNS record values, or free-form log text.
- Same-account, same-region target opens directly. For a different region in the same account, present a confirmable region switch, then load target with the selected profile; `Esc` returns to source context. A different account never opens under current profile; show ARN and require explicit profile switch instead. If active account cannot be verified, disable account-sensitive links.
- Resolve links lazily on selection, with loading/error state. No hidden cross-service scans. Preserve source navigation state and protect against stale link results after profile/region changes.

## Safety and release gate

- Audit every reachable SDK operation against an explicit allowlist: STS `GetCallerIdentity` plus operations in milestones 1 and 2. Add a test or static check that fails when a new AWS call lacks reviewed inventory entry. Reject arbitrary command text; no subprocess execution, generic AWS request builder, write-capable client calls, or mutating convenience APIs.
- Add README install/build/run, credential/profile/region instructions, keyboard reference, service scope, read-only limitations, AWS API cost warning, and recommended least-privilege IAM actions derived from inventory. Explain that application controls and restrictive IAM serve separate safety layers.
- Add runnable offline tests for ARN variants (including partitions), link navigation, cross-region confirmation, cross-account refusal, malformed redrive policy/integration URI, permission failures, terminal control-character escaping, and stale-response cancellation.
- Run formatter, lint, unit tests, and release build. Optional manual smoke check uses read-only AWS credentials: browse all eight services, follow machine-to-Lambda and one other link, switch profile/region, confirm no write request. Do not require credentials in CI.

## Done means

Users can find resources in one active region, inspect details and child views, follow supported static links, and recover from denied or partial reads. No UI path can issue an AWS mutation. No code outside the reviewed read-operation boundary gains general AWS execution capability.

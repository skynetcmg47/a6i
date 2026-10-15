# Active Context

## Current Focus
Full-app release testing: local mock covers all eight service families; live `dudu` now has Step Functions, Lambda, and CloudWatch Logs smoke resources.

## Recent Changes
- Added GitHub Actions release workflow for each successful `main` push: Rust checks, Linux x86_64 binary and SHA-256 checksum, commit-targeted SemVer-tagged GitHub Release. Cargo version remains unchanged.
- Ignored root `.agents/` and `.claude/` directories in `.gitignore`.
- Expanded in-app help into the main pane, shortened view-specific footer hints, added filtered/loaded counts and distinct no-match text, and labeled the full-detail shortcut. Added narrow-terminal mock checks at 80x24 and 60x20.
- Root `Esc` no longer exits; `q` no longer quits. `:exit` and `:quit` are the only in-app exit commands. Updated help, docs, unit checks, and offline PTY smoke check.
- Added Rust binary, Ratatui/Crossterm terminal guard, strict CLI and command grammar, local filtering/navigation state, and SDK STS identity lookup.
- Named profiles use their profile credentials rather than ambient environment credentials; context switches cancel prior requests and reject stale responses.
- Added offline parser/state tests and CLI help/error checks; documented read inventory and milestone limits.
- Added paginated Step Functions and Lambda read adapters, child lists, scrollable details, static machine-to-Lambda links, error recovery, and offline ASL fixtures.
- Added Route 53, API Gateway REST/HTTP/WebSocket, CloudWatch alarms/metrics/logs, SNS, SQS, and EventBridge reads. Scoped children load on demand; CloudWatch log events advance 100 at a time with `n`.
- Escaped terminal control characters in displayed AWS strings, documented exact read calls, and added offline service-family pagination/error/stale fixtures.
- Added structural ARN links for EventBridge targets, API Gateway Lambda integrations, SNS subscription endpoints, SQS redrive targets, Lambda SQS mappings, and cross-region Step Functions tasks; mismatched accounts require explicit profile change.
- Cross-region confirmation loads with the selected profile, verifies the target account again, and restores the source view/config on `Esc` or read failure. Added SDK-call inventory gate and least-privilege IAM guidance.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (20 tests), `cargo build --release`, and offline CLI help/unknown-option checks.
- Repeated all release checks (20 tests passed); live STS verified `dudu` in `ap-southeast-1`. PTY browsing completed for all eight services, API Gateway REST/HTTP/WebSocket lists, CloudWatch alarms/log groups/metric metadata, EventBridge default bus detail/rules. CloudWatch metrics returned 596 rows across pages; local filtering showed an empty match, profile/region changes and invalid commands recovered, and `q` exited with terminal restored.
- Ran `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (20 tests), release build, CLI help/error checks, and a local HTTP/PTY smoke test covering every read-operation family. Added CBOR CloudWatch alarm/metric fixtures, log group/stream/event pagination, Lambda list, and API Gateway REST stages to smoke coverage. Live `dudu` STS and CloudWatch metrics/filter/quit succeeded in `ap-southeast-1`; CloudWatch Logs initially returned zero log groups.
- Deployed Lambda functions `a6i-smoke-20261015-6d808b-first` and `a6i-smoke-20261015-6d808b-second` and Standard state machine `a6i-smoke-20261015-6d808b-flow` in `dudu` account `362209063964`, region `ap-southeast-1`. Three executions (`sample-1` through `sample-3`) succeeded. Both Lambda log groups and the Step Functions log group contain events with three-day retention; functions, workflow, and IAM roles persist until explicitly deleted. Live app PTY check opened the workflow-to-Lambda link and CloudWatch group/stream/events successfully.

## Next Steps
1. Commit Rust source, `Cargo.lock`, release workflow, and docs, then push to `main` to trigger the first GitHub Release; verify the Linux binary and checksum on the release page.
2. Run `target/release/a6i --profile dudu --region ap-southeast-1`; use `:stepfunctions`, `:lambda`, or `:cloudwatch` to inspect smoke resources. Log groups: `/aws/lambda/a6i-smoke-20261015-6d808b-first`, `/aws/lambda/a6i-smoke-20261015-6d808b-second`, and `/aws/vendedlogs/states/a6i-smoke-20261015-6d808b-flow`.
3. Delete smoke resources and their IAM roles when no longer needed. Other service-family live links still need representative resources.

## Active Decisions
- Use Rust plan, not generated generic npm commands in `AGENTS.md`.
- Keep each SDK call within reviewed read-only inventory; link only to a matching verified account, confirming other-region switches.

## Blockers
- Live cross-resource navigation is confirmed for Step Functions-to-Lambda. Other service-family resource links have only offline coverage.

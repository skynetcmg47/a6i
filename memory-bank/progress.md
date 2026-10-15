# Progress

## Completed
- [x] Added Linux x86_64 GitHub Release workflow, checksum, tag calculation self-check, and install instructions (2026-10-15). Live release awaits committed Rust source and a push to `main`.
- [x] Ignored root `.agents/` and `.claude/` directories (2026-10-15).
- [x] Fixed clipped TUI help/footer, unclear filtered empty state, and unscrollable-preview guidance; verified narrow-terminal help, search, and full-detail scrolling with the offline mock (2026-10-15).
- [x] Changed quit handling to explicit `:exit`/`:quit`; root `Esc` stays in app, and `q` has no exit binding (2026-10-15).
- [x] Implementation plans written for foundation, first navigation, service coverage, and cross-links/release.
- [x] Memory bank initialized with project-specific context from plans (2026-10-15).
- [x] Milestone 0: Rust TUI shell, profile/region context, STS-only application read inventory, offline state tests (2026-10-15).
- [x] Milestone 1: Step Functions/Lambda lists, details, child views, static cross-link, offline tests (2026-10-15).
- [x] Milestone 2: six remaining services, lazy child lists, bounded log-event pages, read inventory, offline fixtures (2026-10-15).
- [x] Milestone 3: static cross-service links, cross-region confirmation/return, offline safety audit and tests, IAM guidance, release build (2026-10-15).
- [x] Deployed two test Lambdas, one Step Functions state machine, and three successful executions in `dudu` (`ap-southeast-1`); verified Lambda and Step Functions CloudWatch logs and live app navigation (2026-10-15).

## In Progress
- [ ] No application milestone currently in progress.

## Planned
- [x] Live AWS top-level and available child-view smoke check with `dudu` (2026-10-15): eight services, API Gateway families, CloudWatch views and pagination, EventBridge default bus detail/rules, local filtering, context switches, invalid input, terminal exit.
- [x] Full local HTTP/PTY smoke check with dummy credentials (2026-10-15): eight families, detail and child paths, static links, cross-region confirmation, and log-event pages. Live `dudu` STS, CloudWatch metrics/filter, and clean exit rechecked.
- [x] Live Step Functions detail, Lambda cross-link, and CloudWatch log event navigation against test resources in `dudu`.
- [ ] Optional live links for other service families still need representative resources.

## Known Issues
- `dudu` Lambda and Step Functions links are verified live. Route 53/API Gateway/SNS/SQS still lack representative resources; offline tests cover their links and errors.
- `AGENTS.md` contains generated npm build/test commands that do not match planned Rust stack.

# Tech Context

## Stack
- Rust binary; Ratatui and Crossterm for terminal UI; Tokio for background STS context changes.
- Official AWS SDK for Rust: config, STS, Step Functions, Lambda, Route 53, API Gateway REST/v2, CloudWatch/Logs, SNS, SQS, and EventBridge; serde_json for static ASL links and scoped cursors.
- Cargo manifest and lockfile include SDK crates for milestone 2.

## Development Setup
- Rust toolchain required; run `cargo run -- --help` or `cargo run -- --region us-east-1`.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release` (see `plans/03-links-and-release.md`).
- Tests use fake read clients or fixtures; no live AWS credentials needed.

## Repository Structure
- `plans/`: scope, milestones, acceptance criteria, read-operation inventory.
- `memory-bank/`: project context and progress.
- `README.md`: current CLI usage and read inventory; `src/`: application, SDK boundary, model, and terminal binary.

## Constraints
- One selected region at a time; Route 53 is global.
- No AWS CLI subprocess, generic operation dispatcher, write API, or persisted AWS response data.
- Limit displayed secrets: Lambda environment variable names only; no environment values. Do not log credentials, resource content, or CloudWatch log events.
- Existing `AGENTS.md` lists generic npm commands, but repo currently has no `package.json`; follow Rust plan once implementation begins.

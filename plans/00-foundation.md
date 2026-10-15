# Milestone 0: Foundation and read-only boundary

Status: complete (2026-10-15). Depends on: none. Delivers a usable shell with no resource adapters yet.

## Build

1. Initialize a Rust binary with Ratatui, Crossterm, Tokio, and only SDK/config crates needed by this milestone. Keep terminal restoration reliable on normal exit and panic.
2. Parse `--profile` and `--region` at launch; invalid flags print usage before entering raw terminal mode. Load AWS config through the SDK credential chain. Resolve active account using STS `GetCallerIdentity`; clearly show unavailable identity instead of inventing one.
3. Render service selector, header (account/profile/region), table area, detail pane, breadcrumbs, status/error area, and key help. Empty services can show "not available until next milestone"; do not issue placeholder calls.
4. Implement only the command grammar in `plans/README.md`. `:profile NAME` rebuilds SDK config for that named profile; `:region REGION` rebuilds regional clients. Verify new identity before showing account-sensitive links. Both transitions cancel active fetches, clear resource state, and advance a context generation so stale responses cannot render. Invalid profile/region keeps prior context and shows error. Route 53 ignores active region for requests but still displays it in header.
5. Create one small UI resource shape (service, resource kind, ID/ARN, display fields, detail fields, links) and a typed per-service read adapter boundary. No general SDK method-by-name execution; operations must be explicit methods implemented by read-only clients.
6. Run network fetches away from input/render loop. One view fetch at a time is sufficient. Keep pagination sequential, allow cancellation, and show loading, partial, complete, and failed states. Local `/` filter operates on loaded rows without new AWS calls. Preserve list scroll/filter/selection on detail navigation and `Esc`.

## Read-only inventory

For this milestone permit only STS `GetCallerIdentity`. Later milestone files enumerate additional calls. Do not add generic HTTP, shell, `aws` CLI, or an untyped operation dispatcher. A reviewable inventory in code/docs and test coverage of command dispatch form the safety gate; recommend restrictive IAM credentials in user documentation as defense in depth.

## Acceptance

- `--help` works without credentials or terminal raw mode; interactive startup reports missing credentials/region clearly.
- Service commands and help work; unknown commands cause no AWS call. `q`, `Esc`, and terminal failures restore terminal state.
- Profile/region switch replaces visible identity/context, cancels in-flight work, and cannot show results from old context.
- Search is case-insensitive and local; list loading/partial state remains visible during search.
- Offline tests exercise command whitelist, mode-specific key handling, context generation/stale responses, navigation history, and terminal cleanup where practical.

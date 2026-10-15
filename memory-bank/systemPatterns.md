# System Patterns

## Architecture
- Terminal input/render loop polls every 50 ms; Tokio tasks load and verify new contexts away from input handling.
- `reads.rs` owns reviewed SDK reads for Step Functions/Lambda; `coverage.rs` owns the remaining six service families; `model.rs` holds resource/page types and offline pagination checks.
- Navigation tracks list/filter/selection history; detail links push source views and restore them on back or target read failure.

## Key Patterns
- Sequential pagination: show loading/partial/complete states; preserve loaded rows on page error.
- CloudWatch log events load in pages of at most 100; `n` requests the next page instead of draining unbounded streams.
- Context generation: cancel requests on view/profile/region changes and discard late results from older contexts.
- Static links: parse explicit AWS references; do not infer targets from names or dynamic expressions. Require verified account identity for account-sensitive links.
- Safety boundary: explicit read-operation inventory; reject unknown commands locally and avoid arbitrary AWS execution paths.

## Component Relationships
Input selects view/context; view requests a typed read adapter; adapter performs an approved SDK call; UI receives pages or errors only for current context generation.

## Design Decisions
| Decision | Rationale | Status |
| --- | --- | --- |
| Rust with Ratatui, Crossterm, Tokio, official AWS SDK | Terminal interface with background context verification | Implemented in milestone 0 |
| One active region and lazy child loading | Prevent unrequested scans and constrain costs | Planned in `plans/README.md` |
| Explicit allowlist of read calls | STS and all eight service families in `README.md`; credential chain may perform authentication requests; offline test guards SDK sends | Implemented through milestone 3 |

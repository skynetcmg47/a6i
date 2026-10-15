# Product Context

## Problem Statement
Exploring related AWS resources across multiple service interfaces is slow. Users need a terminal view that does not expose resource mutation controls.

## Solution
Provide a read-only, k9s-like TUI for eight AWS services. Fetch only the selected view, page in the background, and navigate explicit references to supported targets.

## User Experience
- Launch `a6i [--profile NAME] [--region REGION]` with the SDK credential/region chain as fallback.
- Switch services with `:service`, change context with `:profile NAME` or `:region REGION`, filter a loaded list with `/`, open with `Enter`, return with `Esc`, refresh with `r`, show help with `?`, and quit with `q`.
- Show active account/profile/region, loading and partial-result status, and actionable errors. Preserve navigation state when returning from details.
- Never imply a partial list covers the whole account; never issue hidden cross-service scans. AWS read calls may incur charges.

## Success Criteria
- Browse requested services and supported static links without a UI path to AWS write APIs.
- Recover from denied/failed reads without losing previously loaded list rows.
- Run automated tests offline, without live AWS credentials.

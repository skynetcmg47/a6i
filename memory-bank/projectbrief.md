# Project Brief: a6i

## Overview
`a6i` is a read-only, k9s-like terminal browser for AWS resources. Source of truth for implementation scope: `plans/README.md` and its four milestone files. Foundation and Step Functions/Lambda navigation are implemented.

## Core Requirements
- Browse Route 53, API Gateway, Lambda, Step Functions, CloudWatch, SNS, SQS, and EventBridge in one selected AWS region (Route 53 uses its global endpoint).
- List and inspect resources, filter loaded rows locally, and follow supported static references without mutating resources.
- Use the AWS SDK credential chain; support profile and region switches, pagination, partial/error states, cancellation, and stale-response protection.
- Restrict AWS access to an explicit inventory of read operations; keep credentials, resource contents, and log events out of application logs and persistent files.

## Goals
- Provide navigable, recoverable terminal exploration across the eight services.
- Make read-only behavior auditable and verify core behavior with offline tests.

## Scope
### In Scope
- Rust TUI with service, resource, detail, child-view, and cross-resource navigation.
- One active region and account context at a time; STS identity lookup.

### Out of Scope
- AWS mutations, SQS message receives, Lambda invocation/code download, offline snapshots, account-wide or multi-region scans, arbitrary AWS operations, and shell commands.
- Services outside the eight listed above (except STS identity lookup).

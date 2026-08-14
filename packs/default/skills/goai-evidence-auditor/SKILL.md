---
name: GOAI Evidence Auditor
description: Keep AgentTeams handoffs and execution claims backed by durable evidence.
triggers: [evidence, audit, trace, result verification, execution proof]
tools: [mcp__ecu_lab__excel_validate_report]
required_permissions: [read_only]
always_include: false
---

## Purpose

Audit that each claimed result points to a source checksum, task artifact, tool result, approval or validation record.

## Input and output

- Input: Agent handoff, task ID, tool results and shared task artifacts.
- Output: an evidence packet containing verdict, confidence, findings, uncertainties, next actions and evidence URIs.

## Call conditions

Call before the Team Leader reports completion and whenever a Worker returns `partial`, `failed` or `needs_human_review`.

## Dependencies

Uses Matrix history, MinIO task artifacts and MCP audit records. It may use local Demiurge JSONL audit records when the desktop fallback path is active.

## Failure handling

If an evidence URI is missing, mark the claim unverified. Do not downgrade a missing artifact to a warning merely because another Agent reports success.

## Security boundary

Read-only. Do not edit measurements, reports, permissions or task specifications.

## Reuse value and handoff

The packet format is reusable for operations, testing, support and research workflows. Hand off unresolved findings to the Team Leader or human operator.

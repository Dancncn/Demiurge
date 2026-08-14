---
name: GOAI MDA Measurement Capture
description: Execute an approved MDA measurement plan with idempotent evidence-preserving retries.
triggers: [mda, measurement capture, sample values, maximum minimum average]
tools: [mcp__ecu_lab__mda_capture_measurements]
required_permissions: [external, privileged, human_confirmation]
always_include: false
---

## Purpose

Use the normalized catalog to collect raw measurements from an MDA session while preserving the exact scope, duration and evidence needed by the Data Processing Expert.

## Input and output

- Input: `task_id`, `catalog_checksum`, selected signal IDs, ECU IDs, duration and `idempotency_key` from `data-processing-expert`.
- Output: `capture_id`, raw measurement rows, status, timestamps, source references and `evidence_uri`; this Skill does not produce the Excel raw-data workbook.

## Call conditions

Call only after `data-processing-expert` has fixed the catalog and measurement scope. Require human confirmation immediately before an external or desktop action. The confirmation must show target application, ECU count, signal count, duration and affected files.

## Dependencies

Uses the `mda_capture_measurements` MCP tool, the A2L catalog, the shared task directory and the idempotency contract supplied by `data-processing-expert`.

## Failure handling

Retry only with the same idempotency key when the tool reports a transient transport failure. Preserve partial captures, mark them `partial`, and escalate missing or ambiguous signals to the human operator.

## Security boundary

No raw MDA credential may enter the prompt, Matrix message, Skill output or report. The Worker may not widen signal scope without a new approval.

## Reuse value and handoff

The capture contract works for different ECU counts, signal counts and sampling durations. Hand off raw rows and evidence back to `data-processing-expert` for the versioned Excel raw-data artifact. `report-agent` receives that artifact through the shared contract; never hand off only computed aggregates.

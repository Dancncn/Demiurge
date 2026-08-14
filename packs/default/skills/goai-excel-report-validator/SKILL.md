---
name: GOAI Excel Raw-Data Validator
description: Build a versioned Excel raw-data workbook and independently recompute its aggregates.
triggers: [excel, xlsx, raw data, validation, max min average, measurement summary]
tools: [mcp__ecu_lab__excel_build_raw_data, mcp__ecu_lab__excel_validate_report]
required_permissions: [mutating_artifact, read_only]
always_include: false
---

## Purpose

Produce a reviewable Excel raw-data intermediate artifact from captured measurements and verify that every reported maximum, minimum and average can be reproduced before the Report Agent uses it as an input.

## Input and output

- Input: raw measurement rows, catalog checksum, capture ID, raw-data template and acceptance rules.
- Output: versioned `.xlsx` raw-data artifact, workbook checksum, per-ECU/per-signal aggregates, missing-data list and validation evidence.

## Call conditions

Build only after the capture has a stable `capture_id`. Validate after writing a new raw-data workbook version and before `data-processing-expert` hands it to `report-agent`.

## Dependencies

Uses `excel_build_raw_data` and `excel_validate_report` through the Higress MCP gateway. The validator must read raw rows, not trust values copied from a prior workbook or formal trial report.

## Failure handling

On any mismatch, preserve the report and raw capture, return failed checks and create a new version only after correction. Never overwrite a previously accepted report.

## Security boundary

The Skill may write only under the approved task output URI. It must not modify A2L inputs, raw captures or unrelated workspaces.

## Reuse value and handoff

The output contract is applicable to any tabular engineering measurement artifact. Hand off the raw-data checksum and validation verdict to `data-processing-expert`; the shared raw-data contract then makes it available to `report-agent`, `goai-evidence-auditor` and the Team Leader.

---
name: GOAI Task Decomposer
description: Decompose an enterprise measurement task into explicit AgentTeams handoffs.
triggers: [ecu, a2l, mda, measurement, report, task decomposition]
tools: [mcp__ecu_lab__ecu_read_a2l]
required_permissions: [read_only, external]
always_include: false
---

## Purpose

Turn a user request into a bounded task graph for the official AgentTeams `ecu-validation-team`, with explicit handoffs to the three business Workers.

## Input and output

- Input: task description, source document URIs, ECU list, signal scope, duration, report specification and acceptance rules.
- Output: a task specification with `task_id`, ordered or parallel stages, assigned Agent Identity, acceptance criteria, evidence locations and escalation rules.

## Call conditions

Call before any MDA interaction or report write. Dispatch `data-processing-expert`, `compliance-process-agent` and `report-agent` only after their inputs and boundaries are explicit. Do not dispatch a task when the source documents, target ECU scope or acceptance criteria is missing.

## Dependencies

Uses `ecu_read_a2l` for catalog discovery and AgentTeams shared state for the task specification. The Team Leader owns orchestration; business Workers own their defined artifacts.

## Failure handling

Return a structured missing-input list. Do not infer ECU identifiers or signal ranges. A failed parse blocks downstream measurement rather than creating an empty task.

## Security boundary

This Skill may read task metadata but cannot approve desktop actions, access raw credentials or overwrite a previous artifact.

## Reuse value and handoff

The task graph is reusable for other measurement tools and manufacturing validation scenarios. Hand off the approved document URIs, ECU/signal scope, acceptance rules and report obligations to `data-processing-expert`, `compliance-process-agent` and `report-agent`.

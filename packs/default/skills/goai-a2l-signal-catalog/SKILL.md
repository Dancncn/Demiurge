---
name: GOAI A2L Signal Catalog
description: Normalize A2L measurement definitions into a deterministic signal catalog.
triggers: [a2l, signal catalog, message mapping, ecu database]
tools: [mcp__ecu_lab__read_a2l]
required_permissions: [read_only]
always_include: false
---

## Purpose

Parse A2L definitions into stable signal and message records that downstream Workers can use without manually searching the MDA UI.

## Input and output

- Input: `source_uri`, encoding, ECU filter and optional signal filter.
- Output: `source_checksum`, `signals[]`, `messages[]`, parser diagnostics and a deterministic catalog checksum.

## Call conditions

Call once per source checksum. Reuse an existing catalog when the source checksum and parser version are unchanged.

## Dependencies

Requires the `ecu_read_a2l` MCP tool and the shared task directory. The Skill does not access MDA or generate a report.

## Failure handling

Reject malformed or ambiguous records with source location diagnostics. Retry only transport failures; never retry a deterministic parse error as if it were transient.

## Security boundary

Read-only access to approved input URIs. Do not follow arbitrary network locations, expose secrets, or claim that a catalog record has been measured.

## Reuse value and handoff

The normalized schema is independent of MDA, ECU count and vehicle program. Hand off `catalog_checksum`, signal IDs and parser diagnostics to the measurement Worker.

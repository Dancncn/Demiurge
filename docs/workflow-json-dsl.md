# Workflow JSON DSL

Demiurge supports a Rust-native workflow runtime for multi-agent orchestration. Put workflow files in the sandbox under:

```text
.demiurge/workflows/<name>.json
```

Open the top-bar `Workflows` panel to list definitions, start runs, stop active runs, and resume a run by injecting `/workflow resume <run_id>` into chat.

## Example

```json
{
  "name": "agent-review",
  "description": "Explore, critique, and synthesize an agent change",
  "steps": [
    {
      "type": "phase",
      "name": "Explore",
      "steps": [
        {
          "type": "parallel",
          "items": [
            {
              "type": "agent",
              "label": "reader",
              "context_mode": "brief",
              "prompt": "Read the agent module and summarize the current design."
            },
            {
              "type": "agent",
              "label": "critic",
              "context_mode": "fork",
              "prompt": "Find risks, missing tests, and context-engineering weaknesses."
            }
          ]
        }
      ]
    },
    {
      "type": "agent",
      "label": "synthesizer",
      "context_mode": "brief",
      "prompt": "Synthesize the previous findings into a concise implementation plan."
    }
  ]
}
```

## Step Types

- `log`: append a journal/panel log message.
- `phase`: group nested steps under a visible phase name.
- `agent`: run a read-only subagent with `prompt`, optional `label`, optional `agent_type`, and optional `context_mode` (`brief`, `recent`, or `fork`).
- `parallel`: run up to 8 child steps concurrently.
- `pipeline`: run child steps sequentially.
- `budget`: record a budget marker in the journal. Hard enforcement is reserved for a later pass.

Every run writes JSONL events to `.demiurge/workflow-runs/<run_id>/journal.jsonl`.

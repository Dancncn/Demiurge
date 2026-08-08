import type { AgentEventEnvelope, ToolEndEvent, ToolStartEvent } from "@/lib/types";

export type PetSemanticAction =
  | "idle"
  | "thinking"
  | "working"
  | "reviewing"
  | "success"
  | "failure"
  | "movingLeft"
  | "movingRight"
  | "hover"
  | "tap";

export type PetSignal =
  | { type: "agent.idle" }
  | { type: "agent.thinking" }
  | { type: "agent.success" }
  | { type: "agent.failure" }
  | { type: "agent.tool_started"; toolCallId: string; category: "working" | "reviewing" }
  | { type: "agent.tool_finished"; toolCallId: string; ok: boolean }
  | { type: "motion.started"; direction: "left" | "right" }
  | { type: "motion.ended" }
  | { type: "interaction.hover_started" }
  | { type: "interaction.hover_ended" }
  | { type: "interaction.tap" }
  | { type: "animation.completed" };

export interface PetActivitySnapshot {
  action: PetSemanticAction;
}

export class PetActivityState {
  private base: "idle" | "thinking" = "idle";
  private readonly tools = new Map<string, "working" | "reviewing">();
  private reaction: "success" | "failure" | "tap" | null = null;
  private motion: "left" | "right" | null = null;
  private hovered = false;

  dispatch(signal: PetSignal): PetActivitySnapshot {
    switch (signal.type) {
      case "agent.idle":
        this.base = "idle";
        this.tools.clear();
        this.reaction = null;
        break;
      case "agent.thinking":
        this.base = "thinking";
        break;
      case "agent.success":
        this.base = "idle";
        this.tools.clear();
        this.reaction = "success";
        break;
      case "agent.failure":
        this.base = "idle";
        this.tools.clear();
        this.reaction = "failure";
        break;
      case "agent.tool_started":
        this.tools.set(signal.toolCallId, signal.category);
        this.reaction = null;
        break;
      case "agent.tool_finished":
        this.tools.delete(signal.toolCallId);
        this.reaction = signal.ok ? null : "failure";
        break;
      case "motion.started":
        this.motion = signal.direction;
        break;
      case "motion.ended":
        this.motion = null;
        break;
      case "interaction.hover_started":
        this.hovered = true;
        break;
      case "interaction.hover_ended":
        this.hovered = false;
        break;
      case "interaction.tap":
        this.reaction = "tap";
        break;
      case "animation.completed":
        this.reaction = null;
        break;
    }
    return { action: this.currentAction() };
  }

  snapshot(): PetActivitySnapshot {
    return { action: this.currentAction() };
  }

  private currentAction(): PetSemanticAction {
    if (this.motion) return this.motion === "left" ? "movingLeft" : "movingRight";
    if (this.reaction) return this.reaction;
    if (this.hovered) return "hover";
    if ([...this.tools.values()].includes("working")) return "working";
    if (this.tools.size > 0) return "reviewing";
    return this.base;
  }
}

export function classifyTool(event: ToolStartEvent): "working" | "reviewing" {
  if (event.risk && event.risk !== "read_only") return "working";
  if (/write|edit|shell|execute|apply|delete|move|create/i.test(event.name)) return "working";
  return "reviewing";
}

export function signalFromAgentEvent(event: AgentEventEnvelope): PetSignal | null {
  switch (event.kind) {
    case "assistant_start":
    case "assistant_reasoning":
      return { type: "agent.thinking" };
    case "assistant_done":
      return { type: "agent.success" };
    case "assistant_error":
    case "assistant_interrupted":
      return { type: "agent.failure" };
    case "tool_start": {
      const tool = event.payload as ToolStartEvent;
      return {
        type: "agent.tool_started",
        toolCallId: tool.tool_call_id,
        category: classifyTool(tool),
      };
    }
    case "tool_end": {
      const tool = event.payload as ToolEndEvent;
      return { type: "agent.tool_finished", toolCallId: tool.tool_call_id, ok: tool.ok };
    }
    default:
      return null;
  }
}

export const semanticBindingKey: Record<PetSemanticAction, string> = {
  idle: "system.idle",
  thinking: "agent.thinking",
  working: "agent.working",
  reviewing: "agent.reviewing",
  success: "agent.success",
  failure: "agent.failure",
  movingLeft: "motion.left",
  movingRight: "motion.right",
  hover: "interaction.hover",
  tap: "interaction.tap",
};

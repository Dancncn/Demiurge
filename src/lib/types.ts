// 与 Rust 端结构对应的前端类型

export interface FunctionCall {
  name: string;
  arguments: string;
}
export interface ToolCall {
  id: string;
  type: string;
  function: FunctionCall;
}
export interface Message {
  role: string; // system | user | assistant | tool
  content?: string;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
  name?: string;
}

export interface Settings {
  base_url: string;
  api_key: string;
  model: string;
  current_pack: string;
  max_context_chars: number;
  max_input_tokens: number;
  reserved_output_tokens: number;
  auto_memory_enabled: boolean;
}

export interface PackManifest {
  id: string;
  name: string;
  persona: string;
  avatar?: string;
}

export interface SessionMeta {
  id: string;
  title: string;
  updated_at: number;
}
export interface SessionList {
  active: string;
  sessions: SessionMeta[];
}

export type PermissionEffect = "allow" | "deny" | "ask";
export type PermissionScope = "once" | "session" | "project";
export type ToolRisk = "read_only" | "mutating" | "external" | "privileged";
export type ToolConcurrency = "parallel_safe" | "serial_only";

// ---- 后端 emit 的事件载荷 ----
export interface ToolStartEvent {
  tool_call_id: string;
  name: string;
  args: unknown;
  description?: string;
  risk?: ToolRisk;
  permission_effect?: PermissionEffect;
  concurrency?: ToolConcurrency;
}
export interface ToolEndEvent {
  tool_call_id: string;
  name: string;
  ok: boolean;
  result: string;
}
export interface ConfirmRequestEvent {
  id: string;
  tool: string;
  args: string; // 已 pretty 的 JSON 字符串
  description?: string;
  risk?: ToolRisk;
  effect?: PermissionEffect;
  scope?: PermissionScope;
  reason?: string;
  summary?: string;
  preview?: string;
}

// ---- 前端聊天展示项 ----
export type DisplayItem =
  | { id: string; kind: "user"; text: string }
  | { id: string; kind: "assistant"; text: string; streaming: boolean; error?: boolean }
  | {
      id: string;
      kind: "tool";
      tool_call_id?: string;
      name: string;
      args: unknown;
      status: "running" | "done" | "denied";
      result?: string;
      description?: string;
      risk?: ToolRisk;
      permission_effect?: PermissionEffect;
    };

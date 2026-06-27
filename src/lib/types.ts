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

// ---- 后端 emit 的事件载荷 ----
export interface ToolStartEvent {
  name: string;
  args: unknown;
}
export interface ToolEndEvent {
  name: string;
  ok: boolean;
  result: string;
}
export interface ConfirmRequestEvent {
  id: string;
  tool: string;
  args: string; // 已 pretty 的 JSON 字符串
}

// ---- 前端聊天展示项 ----
export type DisplayItem =
  | { id: string; kind: "user"; text: string }
  | { id: string; kind: "assistant"; text: string; streaming: boolean; error?: boolean }
  | {
      id: string;
      kind: "tool";
      name: string;
      args: unknown;
      status: "running" | "done" | "denied";
      result?: string;
    };

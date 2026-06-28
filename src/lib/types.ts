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

export type ProviderKind = "open_ai_compatible" | "local" | "anthropic" | "gemini";
export type WebSearchProvider = "auto" | "bing" | "duckduckgo" | "tavily" | "brave" | "exa";

export interface Settings {
  provider: ProviderKind;
  base_url: string;
  api_key: string;
  model: string;
  current_pack: string;
  max_context_chars: number;
  max_input_tokens: number;
  reserved_output_tokens: number;
  auto_memory_enabled: boolean;
  voice_enabled: boolean;
  voice_stt_backend: string;
  voice_tts_backend: string;
  voice_id: string;
  computer_use_enabled: boolean;
  ocr_model_source: OcrModelSource;
  web_search_provider: WebSearchProvider;
  tavily_api_key: string;
  brave_search_api_key: string;
  exa_api_key: string;
}

export type OcrModelSource = "modelscope" | "huggingface";

export interface OcrModelFileStatus {
  name: string;
  present: boolean;
  bytes: number;
}

export interface OcrModelStatus {
  installed: boolean;
  modelDir: string;
  source: OcrModelSource;
  files: OcrModelFileStatus[];
  missing: string[];
  totalBytes: number;
}

export interface OcrDownloadProgress {
  source: OcrModelSource;
  file: string;
  index: number;
  totalFiles: number;
  downloadedBytes: number;
  totalBytes?: number;
  done: boolean;
}

export interface VoiceStatus {
  enabled: boolean;
  stt_backend: string;
  tts_backend: string;
  voice_id: string;
  ready: boolean;
  reason: string;
}

export type WorkflowStatus = "running" | "done" | "failed" | "killed" | "journaled";

export interface WorkflowDefinitionInfo {
  name: string;
  description: string;
  path: string;
}

export interface WorkflowAgentProgress {
  id: number;
  label: string;
  phase?: string;
  status: WorkflowStatus;
  result?: string;
  error?: string;
}

export interface WorkflowRunProgress {
  run_id: string;
  name: string;
  status: WorkflowStatus;
  current_phase?: string;
  agents: WorkflowAgentProgress[];
  logs: string[];
  journal_path: string;
  started_at: number;
  updated_at: number;
  error?: string;
}

export interface WorkflowPanelState {
  definitions: WorkflowDefinitionInfo[];
  runs: WorkflowRunProgress[];
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
export type PermissionDecisionSource = "tool_default" | "user_override" | "unknown_tool";
export type ToolRisk = "read_only" | "mutating" | "external" | "privileged";
export type ToolConcurrency = "parallel_safe" | "serial_only";

export interface PermissionRuleView {
  tool: string;
  effect: PermissionEffect;
  scope: PermissionScope;
  reason: string;
  updated_at: number;
}

export interface PermissionAuditEntry {
  timestamp: number;
  tool: string;
  effect: PermissionEffect;
  scope: PermissionScope;
  source: PermissionDecisionSource;
  reason: string;
}

export interface PermissionPanelState {
  rules: PermissionRuleView[];
  audit: PermissionAuditEntry[];
}

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

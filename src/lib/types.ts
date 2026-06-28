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

export type ProviderKind =
  | "deepseek"
  | "dashscope"
  | "openai"
  | "openrouter"
  | "anthropic"
  | "gemini"
  | "glm"
  | "minimax"
  | "custom"
  | "open_ai_compatible"
  | "local";
export type PermissionMode = "plan" | "default" | "auto" | "bypass";
export type WebSearchProvider = "auto" | "bing" | "duckduckgo" | "tavily" | "brave" | "exa";

export interface Settings {
  provider: ProviderKind;
  permission_mode: PermissionMode;
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
  webdav_enabled: boolean;
  webdav_url: string;
  webdav_username: string;
  webdav_password: string;
  webdav_path: string;
  media_provider: string;
  media_base_url: string;
  media_api_key: string;
  image_model: string;
  image_size: string;
  tts_model: string;
  tts_voice: string;
}

export interface ImageGenerationRequest {
  prompt: string;
  model?: string;
  size?: string;
  negative_prompt?: string;
  seed?: number;
  prompt_extend?: boolean;
  watermark?: boolean;
}

export interface GeneratedImage {
  url: string;
}

export interface ImageGenerationResult {
  request_id: string;
  images: GeneratedImage[];
  usage: Record<string, unknown>;
}

export interface SpeechSynthesisRequest {
  text: string;
  model?: string;
  voice?: string;
  language_type?: string;
}

export interface SpeechSynthesisResult {
  request_id: string;
  url: string;
  usage: Record<string, unknown>;
}

export interface WebDavConfig {
  url: string;
  username: string;
  password: string;
  path: string;
}

export interface WebDavBackupFile {
  file_name: string;
  modified_time: string;
  size: number;
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

export interface TokenBudgetState {
  total?: number;
  used_exact: number;
  used_estimated: number;
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
  budget: TokenBudgetState;
}

export interface WorkflowPanelState {
  definitions: WorkflowDefinitionInfo[];
  runs: WorkflowRunProgress[];
}

export interface MemoryEntry {
  id: string;
  kind: string;
  text: string;
  line: number;
}

export interface MemoryDuplicateGroup {
  canonical_id: string;
  duplicate_ids: string[];
}

export interface MemoryPanelState {
  path: string;
  entries: MemoryEntry[];
  duplicates: MemoryDuplicateGroup[];
}

export interface ContextPanelState {
  message_count: number;
  user_messages: number;
  assistant_messages: number;
  tool_messages: number;
  summary_chars: number;
  estimated_history_tokens: number;
  max_input_tokens: number;
  reserved_output_tokens: number;
}

export interface PackManifest {
  id: string;
  name: string;
  persona: string;
  avatar?: string;
}

export type AgentKind = "template" | "team";

export interface AgentBudget {
  max_input_tokens?: number;
  reserved_output_tokens?: number;
  max_steps?: number;
  max_total_tokens?: number;
}

export interface AgentDefinitionInfo {
  name: string;
  description: string;
  kind: AgentKind;
  path: string;
  prompt: string;
  allowed_tools: string[];
  invalid_tools: string[];
  budget?: AgentBudget;
  handoff_format: string;
  members: string[];
}

export interface AgentPanelState {
  definitions: AgentDefinitionInfo[];
  agents_dir: string;
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

export interface PlanState {
  active: boolean;
  approved: boolean;
  path?: string;
  content?: string;
  created_at?: number;
  approved_at?: number;
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

export interface GoalProgressEvent {
  status: string;
  message: string;
  turns_executed: number;
  tokens_used: number;
  token_budget?: number;
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

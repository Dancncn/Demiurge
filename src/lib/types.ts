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
  | "xai"
  | "groq"
  | "mistral"
  | "moonshot"
  | "perplexity"
  | "doubao"
  | "hunyuan"
  | "stepfun"
  | "custom"
  | "open_ai_compatible"
  | "local";
export type PermissionMode = "plan" | "default" | "auto" | "bypass";
export type ReasoningEffort = "auto" | "low" | "medium" | "high" | "xhigh" | "max";
export type Language = "zh" | "en";
export type AppTheme = "system" | "light" | "dark";
export type WebSearchProvider = "auto" | "bing" | "duckduckgo" | "tavily" | "brave" | "exa";

export interface ConnectionTestResult {
  ok: boolean;
  target: string;
  detail: string;
  latency_ms: number;
}

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
  /** When true, the input budget follows the selected model's context window. */
  context_budget_auto: boolean;
  /** UI language: "zh" (default) or "en". */
  language: Language;
  theme: AppTheme;
  launch_on_startup: boolean;
  reasoning_effort: ReasoningEffort;
  auto_memory_enabled: boolean;
  companion_enabled: boolean;
  companion_memory_extraction_enabled: boolean;
  companion_memory_extraction_scope: string;
  companion_tone: string;
  companion_mood: string;
  companion_energy: string;
  companion_focus: string;
  companion_do_not_disturb: string;
  weather_enabled: boolean;
  weather_location_mode: string;
  weather_city: string;
  weather_provider: string;
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
  mcp_servers: McpServerConfig[];
}

export interface CompanionPanelState {
  enabled: boolean;
  privacy: CompanionPrivacyState;
  user_state: CompanionUserState;
  weather?: WeatherCard | null;
  weather_cache: WeatherCacheState;
  weather_error?: string | null;
  suggestions: CompanionSuggestion[];
  updated_at: number;
}

export interface CompanionPrivacyState {
  weather_enabled: boolean;
  provider: string;
  location_mode: string;
  city: string;
  note: string;
}

export interface WeatherCacheState {
  entries: number;
  active_city?: string | null;
  active_cached: boolean;
  expires_at?: number | null;
  ttl_ms: number;
  last_error?: string | null;
  location_cached: boolean;
}

export interface CompanionUserState {
  mood: string;
  energy: string;
  focus: string;
  tone: string;
  do_not_disturb: string;
  recent_interaction_at: number;
}

export interface WeatherCard {
  city: string;
  country: string;
  temperature_c: number;
  apparent_temperature_c: number;
  precipitation_mm: number;
  humidity_percent?: number | null;
  wind_speed_kmh?: number | null;
  uv_index?: number | null;
  air_quality_index?: number | null;
  pm2_5?: number | null;
  day_temperature_min_c?: number | null;
  day_temperature_max_c?: number | null;
  commute_precipitation_probability?: number | null;
  severe_weather: boolean;
  weather_code: number;
  condition: string;
  advice: string[];
  source: string;
  cached: boolean;
  fetched_at: number;
}

export interface CompanionSuggestion {
  kind: string;
  priority: number;
  text: string;
}

export type PomodoroMode = "focus" | "short_break" | "long_break" | "custom";
export type PomodoroStatus = "idle" | "running" | "paused";
export type PomodoroTaskKind = "manual" | "session" | "goal" | "workflow";

export interface PomodoroTaskBinding {
  kind: PomodoroTaskKind;
  title: string;
  session_id?: string | null;
  goal_objective?: string | null;
  workflow_run_id?: string | null;
}

export interface PomodoroFeedback {
  start_message?: string | null;
  completion_message?: string | null;
}

export interface PomodoroTimer {
  status: PomodoroStatus;
  mode: PomodoroMode;
  run_id?: string | null;
  duration_secs: number;
  remaining_secs: number;
  started_at?: number | null;
  ends_at?: number | null;
  paused_at?: number | null;
  completed_focus_count: number;
  focus_streak: number;
  task: PomodoroTaskBinding;
  feedback: PomodoroFeedback;
  updated_at: number;
}

export interface PomodoroRhythmMemory {
  focus_sessions_completed: number;
  focus_duration_counts: Record<string, number>;
  interruption_reasons: Record<string, number>;
  efficient_hour_counts: Record<string, number>;
  last_completed_at?: number | null;
}

export interface PomodoroPanelState {
  timer: PomodoroTimer;
  rhythm: PomodoroRhythmMemory;
  remaining_secs: number;
  next_mode: PomodoroMode;
  path: string;
  updated_at: number;
}

export interface PomodoroStartRequest {
  mode: PomodoroMode;
  duration_minutes?: number | null;
  task?: PomodoroTaskBinding | null;
}

export interface PomodoroCompletedEvent {
  title: string;
  body: string;
  state: PomodoroPanelState;
}

export interface CompanionMemorySuggestion {
  id: string;
  kind: string;
  text: string;
  reason: string;
}

export type CompanionMemoryQueueStatus = "pending" | "saved" | "ignored";

export interface CompanionMemoryQueueItem {
  id: string;
  source_session: string;
  reason: string;
  scope: string;
  kind: string;
  text: string;
  created_at: number;
  status: CompanionMemoryQueueStatus;
  saved_memory_id?: string | null;
  duplicate_memory_id?: string | null;
  duplicate_memory_text?: string | null;
}

export interface CompanionMemoryQueueState {
  path: string;
  pending_count: number;
  items: CompanionMemoryQueueItem[];
}

export type McpTransportKind = "stdio";

export interface McpEnvVar {
  key: string;
  value: string;
  secret: boolean;
}

export interface McpServerConfig {
  name: string;
  enabled: boolean;
  transport: McpTransportKind;
  command: string;
  args: string[];
  env: McpEnvVar[];
}

export type McpServerStatus = "disabled" | "pending" | "connected" | "failed";

export interface McpToolView {
  name: string;
  server_name: string;
  original_name: string;
  title?: string;
  description: string;
  risk: ToolRisk;
  read_only: boolean;
  destructive: boolean;
  open_world: boolean;
}

export interface McpResourceView {
  uri: string;
  name?: string;
  description?: string;
  mime_type?: string;
}

export interface McpServerView {
  name: string;
  enabled: boolean;
  transport: McpTransportKind;
  command: string;
  args: string[];
  status: McpServerStatus;
  error?: string;
  server_info?: string;
  instructions?: string;
  tool_count: number;
  resource_count: number;
  updated_at: number;
  stderr_tail?: string;
}

export interface McpPanelState {
  servers: McpServerView[];
  tools: McpToolView[];
  resources: Record<string, McpResourceView[]>;
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
  downloadUrl: string;
}

export interface OcrModelStatus {
  installed: boolean;
  modelDir: string;
  source: OcrModelSource;
  sourceLabel: string;
  sourceNote: string;
  sourceUrl: string;
  files: OcrModelFileStatus[];
  missing: string[];
  totalBytes: number;
  manualInstallHint: string;
}

export interface OcrDownloadProgress {
  source: OcrModelSource;
  sourceLabel: string;
  file: string;
  index: number;
  totalFiles: number;
  completedFiles: number;
  downloadedBytes: number;
  downloadedTotalBytes: number;
  totalBytes?: number;
  phase: "starting" | "downloading" | "finished";
  url: string;
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

export type WorkflowStatus = "running" | "stale_running" | "done" | "failed" | "killed" | "journaled";

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
  cancel_requested: boolean;
  current_phase?: string;
  agents: WorkflowAgentProgress[];
  logs: string[];
  journal_path: string;
  started_at: number;
  updated_at: number;
  error?: string;
  budget: TokenBudgetState;
  steps_total: number;
  steps_done: number;
}

export interface WorkflowPanelState {
  definitions: WorkflowDefinitionInfo[];
  runs: WorkflowRunProgress[];
}

export interface MemoryEntry {
  id: string;
  scope: string;
  scopeLabel: string;
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
  scopes: MemoryScopeState[];
}

export interface MemoryScopeState {
  id: string;
  label: string;
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
  summary_tokens: number;
  system_prompt_chars: number;
  system_prompt_tokens: number;
  estimated_history_tokens: number;
  tools_tokens: number;
  history_budget_tokens: number;
  history_remaining_tokens: number;
  history_over_budget_tokens: number;
  max_input_tokens: number;
  reserved_output_tokens: number;
  input_budget_used_tokens: number;
  input_budget_remaining_tokens: number;
  projected_total_tokens: number;
  prompt_section_tokens: number;
  budget_items: ContextBudgetItem[];
  history_buckets: ContextHistoryBucket[];
  memory_sources: ContextMemorySource[];
  prompt_sections: PromptSectionReport[];
}

export interface ContextBudgetItem {
  id: string;
  label: string;
  tokens: number;
  limit_tokens?: number | null;
  detail: string;
}

export interface ContextHistoryBucket {
  role: string;
  label: string;
  messages: number;
  tokens: number;
}

export interface ContextMemorySource {
  id: string;
  label: string;
  path: string;
  exists: boolean;
  chars: number;
  tokens: number;
  entries: number;
}

export interface PromptSectionReport {
  id: string;
  title: string;
  priority: number;
  chars: number;
  original_chars: number;
  tokens: number;
  included: boolean;
  truncated: boolean;
}

export type GoalStatus =
  | "active"
  | "paused"
  | "blocked"
  | "budget_limited"
  | "usage_limited"
  | "max_turns"
  | "complete";

export interface GoalPanelState {
  objective: string;
  status: GoalStatus;
  status_label: string;
  token_budget: number | null;
  tokens_used: number;
  token_remaining: number | null;
  elapsed: string;
  elapsed_ms: number;
  turns_executed: number;
  max_turns: number;
  blocked_attempts: number;
  last_block_reason: string | null;
  created_at: number;
  updated_at: number;
  can_pause: boolean;
  can_resume: boolean;
  can_continue: boolean;
  can_clear: boolean;
}

export interface PackManifest {
  schema_version?: string;
  id: string;
  name: string;
  description?: string;
  persona: string;
  avatar?: string;
  avatarDataUrl?: string;
  character?: CharacterCard;
  runtime?: CharacterRuntime;
  lorebook?: LoreEntry[];
}

export interface CharacterCard {
  identity?: string;
  background?: string;
  personality?: string[];
  speech_style?: SpeechStyle;
  habits?: string[];
  relationship?: RelationshipStyle;
  opening_messages?: string[];
  example_dialogues?: ExampleDialogue[];
  ooc_rules?: string[];
}

export interface SpeechStyle {
  tone?: string[];
  first_person?: string;
  address_user_as?: string;
  catchphrases?: string[];
  taboo_phrases?: string[];
  sentence_patterns?: string[];
}

export interface RelationshipStyle {
  default?: string;
  progression?: string;
}

export interface ExampleDialogue {
  user: string;
  assistant: string;
}

export interface LoreEntry {
  path: string;
  title?: string;
  tags?: string[];
  priority?: number;
  recursive?: boolean;
  extensions?: string[];
}

export interface CharacterRuntime {
  skills?: SkillBindingPolicy;
  memory?: MemoryPolicy;
  voice?: VoicePreference;
  permissions?: Record<string, string>;
}

export interface SkillBindingPolicy {
  recommended?: string[];
  disabled?: string[];
  auto_activate?: AutoSkillBinding[];
}

export interface AutoSkillBinding {
  skill: string;
  when?: string[];
}

export interface MemoryPolicy {
  namespace?: string;
  write_policy?: string;
  preferred_facts?: string[];
  must_remember?: string[];
  avoid_remembering?: string[];
}

export interface VoicePreference {
  tts_profile?: string;
  speed?: number;
}

export type AgentKind = "template" | "team";

export interface AgentBudget {
  max_input_tokens?: number;
  reserved_output_tokens?: number;
  max_steps?: number;
  max_total_tokens?: number;
}

export interface AgentRuntimeStats {
  run_count: number;
  total_tokens: number;
  error_count: number;
  last_used_at?: number;
  last_error?: string;
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
  runtime: AgentRuntimeStats;
}

export interface AgentPanelState {
  definitions: AgentDefinitionInfo[];
  agents_dir: string;
}

export interface AgentEditorFile {
  name: string;
  file_name: string;
  path: string;
  raw_json: string;
}

export interface AgentValidationResult {
  ok: boolean;
  errors: string[];
  warnings: string[];
  normalized_name: string;
  suggested_file_name: string;
}

export type SkillScope = "global" | "project" | "repository" | "pack" | "compat" | "legacy";

export interface SkillSummary {
  id: string;
  name: string;
  description: string;
  scope: SkillScope;
  path: string;
  triggers: string[];
  declared_tool_needs: string[];
  required_permissions: string[];
  references: string[];
  selected: boolean;
  match_score: number;
}

export interface SkillPanelState {
  skills: SkillSummary[];
  diagnostics: string[];
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

export interface DayCell {
  date: string;
  count: number;
  level: number;
}
export interface StatsPanel {
  sessions: number;
  messages: number;
  est_tokens: number;
  active_days: number;
  current_streak: number;
  longest_streak: number;
  peak_hour: number | null;
  model: string;
  heatmap_days: number;
  heatmap: DayCell[];
}

export type TurnStatus = "running" | "cancelling" | "completed" | "interrupted" | "failed";
export type TurnEntrypoint = "send" | "send_with_agents";

export interface TurnRunState {
  id: string;
  session_id: string;
  entrypoint: TurnEntrypoint;
  status: TurnStatus;
  input_preview: string;
  workflow_run_id?: string;
  agent_names: string[];
  started_at: number;
  updated_at: number;
  completed_at?: number;
  error?: string;
}

export interface SessionEnginePanelState {
  busy: boolean;
  cancel_requested: boolean;
  active_turn?: TurnRunState;
  last_turn?: TurnRunState;
}

export interface TurnEventContext {
  id: string;
  session_id: string;
  status: TurnStatus;
}

export interface AgentEventEnvelope<T = unknown> {
  kind: string;
  turn?: TurnEventContext;
  timestamp: number;
  payload: T;
}

export type PermissionEffect = "allow" | "deny" | "ask";
export type PermissionScope = "once" | "session" | "project" | "user";
export type PermissionDecisionSource = "tool_default" | "user_override" | "unknown_tool";
export type ToolRisk = "read_only" | "mutating" | "external" | "privileged";
export type ToolConcurrency = "parallel_safe" | "serial_only";
export type ToolOutputPolicy = "inline" | "truncate_for_ui";

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

export interface PermissionToolView {
  tool: string;
  description: string;
  risk: ToolRisk;
  default_effect: PermissionEffect;
  default_scope: PermissionScope;
  default_reason: string;
}

export interface PermissionRuleInput {
  tool: string;
  effect: PermissionEffect;
  scope: PermissionScope;
  reason: string;
}

export interface PermissionPanelState {
  rules: PermissionRuleView[];
  audit: PermissionAuditEntry[];
  tools: PermissionToolView[];
}

export interface ShellPolicyState {
  platform: string;
  default_isolation: string;
  strict_timeout_secs: number;
  max_timeout_secs: number;
  env_allowlist: string[];
  strict_blocked_risks: ShellRiskView[];
  risk_rules: ShellRiskRuleView[];
  containment: ShellContainmentView;
}

export interface ShellRiskView {
  id: string;
  label: string;
  severity: string;
}

export interface ShellRiskRuleView {
  class: ShellRiskView;
  reason: string;
  patterns: string[];
  blocked_in_strict: boolean;
}

export interface ShellContainmentView {
  process_group: boolean;
  kill_process_tree_on_timeout: boolean;
  filesystem_sandbox: string;
  network_sandbox: string;
  notes: string[];
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
  output_policy?: ToolOutputPolicy;
  preview?: string;
  affected_paths?: string[];
}
export interface ToolEndEvent {
  tool_call_id: string;
  name: string;
  ok: boolean;
  denied?: boolean;
  result: string;
  duration_ms?: number;
  error_hint?: string;
  source_quality?: ToolSourceQuality;
}

export interface ToolSourceQuality {
  level: "strong" | "limited" | "none";
  source_count: number;
  hint: string;
}
export interface ConfirmRequestEvent {
  id: string;
  tool: string;
  args: string; // 已 pretty 的 JSON 字符串
  description?: string;
  risk?: ToolRisk;
  effect?: PermissionEffect;
  scope?: PermissionScope;
  source?: PermissionDecisionSource;
  reason?: string;
  summary?: string;
  preview?: string;
  affected_paths?: string[];
}

export interface GoalProgressEvent {
  status: string;
  message: string;
  turns_executed: number;
  tokens_used: number;
  token_budget?: number;
}

export interface AssistantErrorEvent {
  kind: "llm" | "network" | "tool" | "workflow" | "unknown";
  message: string;
  hint: string;
  retryable: boolean;
}

// ---- 前端聊天展示项 ----
export type DisplayItem =
  | { id: string; kind: "user"; text: string }
  | {
      id: string;
      kind: "assistant";
      text: string;
      reasoning?: string;
      streaming: boolean;
      error?: boolean;
      errorTitle?: string;
      errorHint?: string;
      retryText?: string;
    }
  | {
      id: string;
      kind: "tool";
      tool_call_id?: string;
      name: string;
      args: unknown;
      status: "running" | "done" | "denied" | "failed";
      result?: string;
      preview?: string;
      description?: string;
      risk?: ToolRisk;
      permission_effect?: PermissionEffect;
      duration_ms?: number;
      error_hint?: string;
      source_quality?: ToolSourceQuality;
    };

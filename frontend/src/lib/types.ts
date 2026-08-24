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
  images?: Array<{ mime_type: string; data: string; name?: string }>;
  context?: ConversationContext;
}

export interface ConversationContext {
  scene: string;
  channel?: string;
  speaker?: string;
  is_primary_user?: boolean;
  addressed_to_ai?: boolean;
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
export type ModelTier = "haiku" | "sonnet" | "opus";
export type Language = "zh" | "en";
export type AppTheme = "system" | "light" | "dark";
export type AppAppearance = "material_bloom" | "classic";
export type WebSearchProvider = "auto" | "bing" | "duckduckgo" | "tavily" | "brave" | "exa";

export interface ModelRoutingConfig {
  enabled: boolean;
  subagent_tier: ModelTier;
  docs_agent_tier: ModelTier;
  haiku_model: string;
  sonnet_model: string;
  opus_model: string;
  fallback_models: string[];
  auto_failover: boolean;
  max_failover_attempts: number;
  failure_threshold: number;
  cooldown_seconds: number;
}

export interface ConnectionTestResult {
  ok: boolean;
  target: string;
  detail: string;
  latency_ms: number;
}

export interface EmbeddingProbeResult {
  ok: boolean;
  dims: number;
  latency_ms: number;
  detail: string;
}

export interface Settings {
  provider: ProviderKind;
  permission_mode: PermissionMode;
  base_url: string;
  api_key: string;
  model: string;
  vision_model: string;
  current_pack: string;
  current_pet: string;
  max_context_chars: number;
  max_input_tokens: number;
  reserved_output_tokens: number;
  /** When true, the input budget follows the selected model's context window. */
  context_budget_auto: boolean;
  /** UI language: "zh" (default) or "en". */
  language: Language;
  theme: AppTheme;
  appearance: AppAppearance;
  launch_on_startup: boolean;
  reasoning_effort: ReasoningEffort;
  model_routing: ModelRoutingConfig;
  auto_memory_enabled: boolean;
  embedding_enabled: boolean;
  embedding_provider: string;
  embedding_base_url: string;
  embedding_api_key: string;
  embedding_model: string;
  embedding_dims: number;
  hybrid_weight: number;
  companion_enabled: boolean;
  companion_memory_extraction_enabled: boolean;
  companion_memory_extraction_scope: string;
  companion_tone: string;
  companion_mood: string;
  companion_energy: string;
  companion_focus: string;
  companion_do_not_disturb: string;
  desktop_companion_enabled: boolean;
  desktop_companion_always_on_top: boolean;
  desktop_companion_click_through: boolean;
  desktop_companion_collapsed: boolean;
  weather_enabled: boolean;
  weather_location_mode: string;
  weather_city: string;
  weather_provider: string;
  voice_enabled: boolean;
  voice_stt_backend: string;
  voice_tts_backend: string;
  voice_id: string;
  voice_speed: number;
  voice_emotion: string;
  voice_streaming: boolean;
  voice_tts_fallback: boolean;
  voice_hotkey_enabled: boolean;
  voice_hotkey: string;
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
  plan_steps: string[];
  completion_message?: string | null;
  recap_prompt?: string | null;
  encouragement?: string | null;
}

export interface PomodoroTimer {
  status: PomodoroStatus;
  mode: PomodoroMode;
  run_id?: string | null;
  duration_secs: number;
  remaining_secs: number;
  started_at?: number | null;
  started_hour?: number | null;
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
  local_hour?: number | null;
}

export interface PomodoroSkipRequest {
  reason?: string | null;
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
  speed?: number;
  emotion?: string;
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

export interface WorkspaceState {
  path: string;
  name: string;
  is_git: boolean;
  branch?: string | null;
  dirty: boolean;
}

/** A direct child of a workspace directory. Paths are always workspace-relative. */
export interface WorkspaceEntry {
  name: string;
  path: string;
  kind: "file" | "directory";
  size: number;
}

/** Text preview returned by the desktop backend. Large files are intentionally capped. */
export interface WorkspaceFilePreview {
  path: string;
  name: string;
  content: string;
  size: number;
  truncated: boolean;
  binary: boolean;
}

export interface GitBranch {
  name: string;
  current: boolean;
  remote: boolean;
  upstream?: string | null;
}

export interface GitChangedFile {
  path: string;
  status: string;
  staged: boolean;
  working_tree: boolean;
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
  tts_ready: boolean;
  tts_reason: string;
  speed: number;
  emotion: string;
  streaming: boolean;
  fallback_enabled: boolean;
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
  namespace?: string;
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
  live2d?: string;
  character?: CharacterCard;
  runtime?: CharacterRuntime;
  lorebook?: LoreEntry[];
  credits?: AssetCredit[];
  license?: string;
}

export interface InstalledPet {
  id: string;
  name: string;
  version: string;
  description?: string;
  thumbnailPath?: string;
}

export interface PetFrameRange {
  row: number;
  start: number;
  count: number;
}

export interface PetAction {
  frames: PetFrameRange;
  frameDurationMs: number;
  mode: "loop" | "once" | "hold";
  returnTo?: string;
}

export interface PetBinding {
  action: string;
  sound?: string;
}

export interface PetSound {
  source: string;
  volume: number;
  loop?: boolean;
}

export interface PetManifest {
  schemaVersion: number;
  id: string;
  name: string;
  version: string;
  description?: string;
  thumbnail?: string;
  renderer: {
    type: "sprite-sheet";
    sheet: string;
    frameWidth: number;
    frameHeight: number;
    columns: number;
  };
  actions: Record<string, PetAction>;
  bindings: Record<string, PetBinding>;
  sounds: Record<string, PetSound>;
}

export interface ResolvedPet {
  manifest: PetManifest;
  sheetPath: string;
  soundPaths: Record<string, string>;
}

export interface AssetCredit {
  asset: string;
  author?: string;
  source?: string;
  license?: string;
}

export interface PackFileEntry {
  path: string;
  is_dir: boolean;
  size: number;
  modified_ms: number;
}

export interface PackFileContent {
  path: string;
  text?: string;
  data_url?: string;
  truncated: boolean;
}

export interface Live2DAsset {
  path: string;
  mime: string;
  data: string;
}

export interface Live2DBundle {
  model_json: string;
  assets: Live2DAsset[];
}

export interface PackLoreFile {
  name: string;
  bytes: number[];
}

export interface PackImportResult {
  manifest: PackManifest;
  warnings: string[];
}

export interface LoreIndexStatus {
  pack_id: string;
  cache_exists: boolean;
  version?: number;
  file_count: number;
  chunk_count: number;
  files_stale: boolean;
  last_built_ms: number;
}

export interface LoreHitDetail {
  score: number;
  source: string;
  title: string;
  heading?: string;
  chunk_index: number;
  text: string;
  tags: string[];
  keywords: string[];
  priority: number;
  matched_terms: string[];
  dense_score?: number;
}

export interface LoreRecallDetail {
  query: string;
  total_chunks: number;
  hits: LoreHitDetail[];
  /** 向量召回状态：disabled / ok / degraded */
  embedding_status: string;
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
  model?: string;
  model_tier?: ModelTier;
  scope?: "read_only" | "docs_write";
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

export type ExternalProvider = "codex" | "claude";

export interface SkillCandidate {
  id: string;
  name: string;
  description: string;
  source: string;
  path: string;
  managed: boolean;
  enabled: boolean;
  imported_at?: number;
}

export interface ExternalSessionMeta {
  id: string;
  provider: ExternalProvider;
  title: string;
  summary?: string;
  project_dir?: string;
  created_at?: number;
  last_active_at?: number;
  source_path: string;
  message_count: number;
}

export interface ExternalSessionMessage {
  role: string;
  content: string;
  timestamp?: number;
}

export interface ExternalConfigSummary {
  provider: ExternalProvider;
  kind: string;
  path: string;
  available: boolean;
  safe_keys: string[];
}

export interface ImportedConfig {
  provider: ExternalProvider;
  kind: string;
  source_path: string;
  destination_path: string;
  safe_values: Record<string, string>;
  imported_at: number;
}

export interface IntegrationSnapshot {
  skills: SkillCandidate[];
  sessions: ExternalSessionMeta[];
  configs: ExternalConfigSummary[];
  diagnostics: string[];
}

export interface MarketSkill {
  key: string;
  name: string;
  directory: string;
  repo_owner: string;
  repo_name: string;
  repo_branch: string;
  installs: number;
  readme_url?: string;
}

export interface MarketSearchResult {
  skills: MarketSkill[];
  total_count: number;
  query: string;
}

export interface ImportedSession {
  id: string;
  title: string;
  provider: string;
  source_path: string;
  message_count: number;
}

export interface UsageBucket {
  key: string;
  requests: number;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  cost_usd: number;
}

export interface UsageRecord {
  id: string;
  session_id: string;
  provider: string;
  model: string;
  purpose: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  cost_usd?: number | null;
  pricing_source?: string | null;
  latency_ms: number;
  status: string;
  created_at: number;
}

export interface UsageSummary {
  total_requests: number;
  successful_requests: number;
  failed_requests: number;
  interrupted_requests: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_tokens: number;
  cache_hit_rate: number;
  average_latency_ms: number;
  total_cost_usd: number;
  priced_requests: number;
  unpriced_requests: number;
  providers: UsageBucket[];
  models: UsageBucket[];
  daily: UsageBucket[];
  recent_records: UsageRecord[];
  log_read_errors?: number;
  malformed_lines?: number;
  oversized_lines?: number;
  write_failures?: number;
}

export interface ModelCatalogEntry {
  id: string;
  name: string;
  context_length: number;
  input_cost_per_million?: number;
  output_cost_per_million?: number;
  cache_read_cost_per_million?: number;
  cache_write_cost_per_million?: number;
  supported_parameters: string[];
  source: string;
  updated_at: number;
}

export interface ModelCatalog {
  source: string;
  fetched_at?: number;
  models: ModelCatalogEntry[];
}

export interface SessionMeta {
  id: string;
  title: string;
  updated_at: number;
  workspace_path?: string;
  workspace_name?: string;
  archived: boolean;
  archived_at?: number;
}
export interface SessionList {
  active: string;
  sessions: SessionMeta[];
}

export interface NavigationSnapshot {
  session_id: string;
  sessions: SessionMeta[];
  history: Message[];
  workspace: WorkspaceState;
  goal: GoalPanelState | null;
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
export type PermissionDecisionSource =
  | "tool_default"
  | "user_override"
  | "unknown_tool"
  | "card_overlay";
export type ToolRisk = "read_only" | "mutating" | "external" | "privileged";
export type ToolConcurrency = "parallel_safe" | "serial_only";
export type ToolOutputPolicy = "inline" | "truncate_for_ui";

export interface PermissionRuleView {
  tool: string;
  effect: PermissionEffect;
  scope: PermissionScope;
  reason: string;
  updated_at: number;
  session_id?: string;
  workspace_identity?: string;
}

export interface PermissionAuditEntry {
  timestamp: number;
  tool: string;
  effect: PermissionEffect;
  scope: PermissionScope;
  source: PermissionDecisionSource;
  reason: string;
  session_id?: string;
  workspace_identity?: string;
}

export interface PermissionToolView {
  tool: string;
  description: string;
  risk: ToolRisk;
  default_effect: PermissionEffect;
  default_scope: PermissionScope;
  default_reason: string;
  card_preference?: string;
}

export interface PermissionRuleInput {
  tool: string;
  effect: PermissionEffect;
  scope: PermissionScope;
  reason: string;
  session_id?: string;
  workspace_identity?: string;
}

export interface PermissionPanelState {
  rules: PermissionRuleView[];
  audit: PermissionAuditEntry[];
  tools: PermissionToolView[];
  notices: string[];
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
  session_id: string;
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
  session_id: string;
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
  | { id: string; kind: "user"; text: string; context?: ConversationContext }
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
      affected_paths?: string[];
      description?: string;
      risk?: ToolRisk;
      permission_effect?: PermissionEffect;
      duration_ms?: number;
      error_hint?: string;
      source_quality?: ToolSourceQuality;
    };

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentEventEnvelope,
  AgentPanelState,
  AgentEditorFile,
  AgentValidationResult,
  AssistantErrorEvent,
  ConnectionTestResult,
  ConfirmRequestEvent,
  CompanionMemoryQueueState,
  CompanionPanelState,
  CompanionMemorySuggestion,
  ContextPanelState,
  GoalPanelState,
  GoalProgressEvent,
  ImageGenerationRequest,
  ImageGenerationResult,
  Message,
  MemoryPanelState,
  McpPanelState,
  OcrDownloadProgress,
  OcrModelSource,
  OcrModelStatus,
  PackManifest,
  PermissionMode,
  PermissionScope,
  PlanState,
  PermissionPanelState,
  PermissionRuleInput,
  SessionEnginePanelState,
  SessionList,
  Settings,
  SkillPanelState,
  StatsPanel,
  ShellPolicyState,
  SpeechSynthesisRequest,
  SpeechSynthesisResult,
  ToolEndEvent,
  ToolStartEvent,
  WebDavBackupFile,
  WebDavConfig,
  VoiceStatus,
  WorkflowPanelState,
} from "./types";

// ---- 命令 ----
export const send = (text: string) => invoke<void>("send", { text });
export const sendWithAgents = (text: string, agentNames: string[]) =>
  invoke<void>("send_with_agents", { text, agentNames });
export const interrupt = () => invoke<void>("interrupt");
export const sessionEngineState = () => invoke<SessionEnginePanelState>("session_engine_state");
export const respondConfirm = (id: string, allow: boolean, scope: PermissionScope) =>
  invoke<void>("respond_confirm", { id, allow, scope });
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const providerCheckConnection = (settings: Settings) =>
  invoke<ConnectionTestResult>("provider_check_connection", { settings });
export const webSearchCheckConnection = (settings: Settings, provider?: string) =>
  invoke<ConnectionTestResult>("web_search_check_connection", { settings, provider });
export const setPermissionMode = (mode: PermissionMode) => invoke<Settings>("set_permission_mode", { mode });
export const planState = () => invoke<PlanState>("plan_state");
export const approvePlan = () => invoke<PlanState>("approve_plan");
export const rejectPlan = () => invoke<PlanState>("reject_plan");
export const permissionPanelState = () => invoke<PermissionPanelState>("permission_panel_state");
export const shellPolicyState = () => invoke<ShellPolicyState>("shell_policy_state");
export const permissionResetRule = (scope: PermissionScope, tool: string) =>
  invoke<PermissionPanelState>("permission_reset_rule", { scope, tool });
export const permissionUpsertRule = (input: PermissionRuleInput) =>
  invoke<PermissionPanelState>("permission_upsert_rule", { input });
export const mcpPanelState = () => invoke<McpPanelState>("mcp_panel_state");
export const mcpRefresh = () => invoke<McpPanelState>("mcp_refresh");
export const mcpSetServerEnabled = (name: string, enabled: boolean) =>
  invoke<McpPanelState>("mcp_set_server_enabled", { name, enabled });
export const listPacks = () => invoke<PackManifest[]>("list_packs");
export const importPackZip = (fileName: string, bytes: number[]) =>
  invoke<PackManifest>("import_pack_zip", { fileName, bytes });
export const readPackManifestJson = (id: string) => invoke<string>("read_pack_manifest_json", { id });
export const savePackManifestJson = (id: string, rawJson: string) =>
  invoke<PackManifest>("save_pack_manifest_json", { id, rawJson });
export const previewPackLorebook = (id: string, query: string) =>
  invoke<string>("preview_pack_lorebook", { id, query });
export const agentPanelState = () => invoke<AgentPanelState>("agent_panel_state");
export const agentTemplateJson = () => invoke<string>("agent_template_json");
export const agentValidateJson = (rawJson: string) => invoke<AgentValidationResult>("agent_validate_json", { rawJson });
export const agentReadFile = (name: string) => invoke<AgentEditorFile>("agent_read_file", { name });
export const agentSaveFile = (fileName: string, rawJson: string) =>
  invoke<AgentPanelState>("agent_save_file", { fileName, rawJson });
export const agentDeleteFile = (name: string) => invoke<AgentPanelState>("agent_delete_file", { name });
export const goalPanelState = () => invoke<GoalPanelState | null>("goal_panel_state");
export const goalPause = () => invoke<GoalPanelState | null>("goal_pause");
export const goalResume = () => invoke<GoalPanelState | null>("goal_resume");
export const goalContinue = () => invoke<GoalPanelState | null>("goal_continue");
export const goalClear = () => invoke<GoalPanelState | null>("goal_clear");
export const getHistory = () => invoke<Message[]>("get_history");
export const contextPanelState = () => invoke<ContextPanelState>("context_panel_state");
export const companionPanelState = () => invoke<CompanionPanelState>("companion_panel_state");
export const companionClearWeatherCache = () =>
  invoke<CompanionPanelState>("companion_clear_weather_cache");
export const companionMemorySuggestions = () =>
  invoke<CompanionMemorySuggestion[]>("companion_memory_suggestions");
export const companionMemoryQueueState = () =>
  invoke<CompanionMemoryQueueState>("companion_memory_queue_state");
export const companionEnqueueMemorySuggestion = (id: string) =>
  invoke<CompanionMemoryQueueState>("companion_enqueue_memory_suggestion", { id });
export const companionSaveMemoryQueueItem = (id: string, resolution?: "merge" | "replace" | "keep_new") =>
  invoke<CompanionMemoryQueueState>("companion_save_memory_queue_item", { id, resolution: resolution ?? null });
export const companionIgnoreMemoryQueueItem = (id: string) =>
  invoke<CompanionMemoryQueueState>("companion_ignore_memory_queue_item", { id });
export const companionSaveAllMemoryQueueItems = () =>
  invoke<CompanionMemoryQueueState>("companion_save_all_memory_queue_items");
export const companionIgnoreAllMemoryQueueItems = () =>
  invoke<CompanionMemoryQueueState>("companion_ignore_all_memory_queue_items");
export const companionUndoMemoryQueueItem = (id: string) =>
  invoke<CompanionMemoryQueueState>("companion_undo_memory_queue_item", { id });

// 技能 / 检索面板：可选 query 用于按输入对技能做匹配检索打分。
export const skillPanelState = (query?: string) =>
  invoke<SkillPanelState>("skill_panel_state", { query: query ?? null });
export const openSkillsDir = () => invoke<void>("open_skills_dir");
export const memoryPanelState = () => invoke<MemoryPanelState>("memory_panel_state");
export const memoryAddEntry = (scope: string, kind: string, text: string) =>
  invoke<MemoryPanelState>("memory_add_entry", { scope, kind, text });
export const memoryUpdateEntry = (id: string, kind: string, text: string) =>
  invoke<MemoryPanelState>("memory_update_entry", { id, kind, text });
export const memoryDeleteEntry = (id: string) => invoke<MemoryPanelState>("memory_delete_entry", { id });
export const memoryDedupeApply = () => invoke<MemoryPanelState>("memory_dedupe_apply");
export const openSandbox = () => invoke<void>("open_sandbox");
export const webdavCheckConnection = (config: WebDavConfig) =>
  invoke<string>("webdav_check_connection", { config });
export const webdavBackupNow = (config: WebDavConfig) => invoke<string>("webdav_backup_now", { config });
export const webdavListBackups = (config: WebDavConfig) =>
  invoke<WebDavBackupFile[]>("webdav_list_backups", { config });
export const webdavDeleteBackup = (config: WebDavConfig, fileName: string) =>
  invoke<void>("webdav_delete_backup", { config, fileName });

// 会话管理
export const listSessions = () => invoke<SessionList>("list_sessions");
export const sessionStats = (offset: number) => invoke<StatsPanel>("session_stats", { offset });
export const newSession = () => invoke<string>("new_session");
export const selectSession = (id: string) => invoke<void>("select_session", { id });
export const deleteSession = (id: string) => invoke<string>("delete_session", { id });
export const renameSession = (id: string, title: string) => invoke<string>("rename_session", { id, title });

// Voice API placeholders. These commands intentionally return a clear
// "backend not implemented" error until a concrete STT/TTS provider is chosen.
export const voiceStatus = () => invoke<VoiceStatus>("voice_status");
export const voiceTranscribe = (audio: number[], mimeType?: string, language?: string) =>
  invoke<string>("voice_transcribe", { audio, mimeType, language });
export const voiceSynthesize = (text: string, voiceId?: string) =>
  invoke<string>("voice_synthesize", { text, voiceId });

export const ocrModelStatus = () => invoke<OcrModelStatus>("ocr_model_status");
export const ocrImageBytes = (bytes: number[]) => invoke<string>("ocr_image_bytes", { bytes });
export const ocrDownloadModels = (source: OcrModelSource) =>
  invoke<OcrModelStatus>("ocr_download_models", { source });
export const listenOcrDownloadProgress = (handler: (e: OcrDownloadProgress) => void) =>
  listen<OcrDownloadProgress>("ocr-download-progress", (e) => handler(e.payload));

export const mediaGenerateImage = (request: ImageGenerationRequest) =>
  invoke<ImageGenerationResult>("media_generate_image", { request });
export const mediaSynthesizeSpeech = (request: SpeechSynthesisRequest) =>
  invoke<SpeechSynthesisResult>("media_synthesize_speech", { request });

export const workflowPanelState = () => invoke<WorkflowPanelState>("workflow_panel_state");
export const workflowRun = (name: string) => invoke<string>("workflow_run", { name });
export const workflowStop = (runId: string) => invoke<void>("workflow_stop", { runId });
export const listenWorkflowUpdated = (handler: (e: WorkflowPanelState) => void) =>
  listen<WorkflowPanelState>("workflow-updated", (e) => handler(e.payload));
export const listenPlanUpdated = (handler: (e: PlanState) => void) =>
  listen<PlanState>("plan-updated", (e) => handler(e.payload));
export const listenPermissionModeUpdated = (handler: (e: PermissionMode) => void) =>
  listen<PermissionMode>("permission-mode-updated", (e) => handler(e.payload));
export const listenSettingsUpdated = (handler: (e: Settings) => void) =>
  listen<Settings>("settings-updated", (e) => handler(e.payload));
export const listenMcpUpdated = (handler: (e: McpPanelState) => void) =>
  listen<McpPanelState>("mcp-updated", (e) => handler(e.payload));
export const listenSessionEngineUpdated = (handler: (e: SessionEnginePanelState) => void) =>
  listen<SessionEnginePanelState>("session-engine-updated", (e) => handler(e.payload));
export const listenUnifiedAgentEvents = (handler: (e: AgentEventEnvelope) => void) =>
  listen<AgentEventEnvelope>("agent-event", (e) => handler(e.payload));

// ---- 事件订阅 ----
export interface AgentEventHandlers {
  onAssistantStart: () => void;
  onAssistantDelta: (text: string) => void;
  onAssistantReasoning?: (text: string) => void;
  onAssistantDone: (text: string) => void;
  onAssistantError: (e: AssistantErrorEvent) => void;
  onAssistantInterrupted: () => void;
  onToolStart: (e: ToolStartEvent) => void;
  onToolEnd: (e: ToolEndEvent) => void;
  onConfirmRequest: (e: ConfirmRequestEvent) => void;
  onGoalProgress: (e: GoalProgressEvent) => void;
}

/// 注册所有 agent 事件监听，返回一个反注册函数。
export async function listenAgentEvents(h: AgentEventHandlers): Promise<UnlistenFn> {
  const uns: UnlistenFn[] = await Promise.all([
    listen("assistant-start", () => h.onAssistantStart()),
    listen<string>("assistant-delta", (e) => h.onAssistantDelta(e.payload)),
    listen<string>("assistant-reasoning", (e) => h.onAssistantReasoning?.(e.payload)),
    listen<string>("assistant-done", (e) => h.onAssistantDone(e.payload)),
    listen<AssistantErrorEvent>("assistant-error", (e) => h.onAssistantError(e.payload)),
    listen("assistant-interrupted", () => h.onAssistantInterrupted()),
    listen<ToolStartEvent>("tool-start", (e) => h.onToolStart(e.payload)),
    listen<ToolEndEvent>("tool-end", (e) => h.onToolEnd(e.payload)),
    listen<ConfirmRequestEvent>("tool-confirm-request", (e) => h.onConfirmRequest(e.payload)),
    listen<GoalProgressEvent>("goal-progress", (e) => h.onGoalProgress(e.payload)),
  ]);
  return () => uns.forEach((u) => u());
}

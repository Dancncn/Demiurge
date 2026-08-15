import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentEventEnvelope,
  AgentPanelState,
  AgentEditorFile,
  AgentValidationResult,
  AssistantErrorEvent,
  ConnectionTestResult,
  EmbeddingProbeResult,
  ConfirmRequestEvent,
  CompanionMemoryQueueState,
  CompanionPanelState,
  CompanionMemorySuggestion,
  ContextPanelState,
  GoalPanelState,
  GoalProgressEvent,
  ImageGenerationRequest,
  ImageGenerationResult,
  InstalledPet,
  Live2DBundle,
  Message,
  LoreIndexStatus,
  LoreRecallDetail,
  MemoryPanelState,
  McpPanelState,
  NavigationSnapshot,
  OcrDownloadProgress,
  OcrModelSource,
  OcrModelStatus,
  PackManifest,
  PackFileContent,
  PackFileEntry,
  PackImportResult,
  PackLoreFile,
  PermissionMode,
  PermissionScope,
  PlanState,
  PomodoroCompletedEvent,
  PomodoroPanelState,
  PomodoroSkipRequest,
  PomodoroStartRequest,
  PermissionPanelState,
  PermissionRuleInput,
  SessionEnginePanelState,
  SessionList,
  Settings,
  ResolvedPet,
  SkillPanelState,
  IntegrationSnapshot,
  SkillCandidate,
  ExternalSessionMessage,
  ImportedSession,
  ImportedConfig,
  MarketSearchResult,
  MarketSkill,
  UsageSummary,
  ModelCatalog,
  StatsPanel,
  ShellPolicyState,
  SpeechSynthesisRequest,
  SpeechSynthesisResult,
  ToolEndEvent,
  ToolStartEvent,
  TurnEventContext,
  GitBranch,
  GitChangedFile,
  WebDavBackupFile,
  WebDavConfig,
  VoiceStatus,
  WorkspaceEntry,
  WorkspaceFilePreview,
  WorkspaceState,
  WorkflowPanelState,
} from "@/lib/types";

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
export const permissionResetRule = (
  scope: PermissionScope,
  tool: string,
  sessionId?: string,
  workspaceIdentity?: string,
) =>
  invoke<PermissionPanelState>("permission_reset_rule", {
    scope,
    tool,
    sessionId: sessionId ?? null,
    workspaceIdentity: workspaceIdentity ?? null,
  });
export const permissionUpsertRule = (input: PermissionRuleInput) =>
  invoke<PermissionPanelState>("permission_upsert_rule", { input });
export const mcpPanelState = () => invoke<McpPanelState>("mcp_panel_state");
export const mcpRefresh = () => invoke<McpPanelState>("mcp_refresh");
export const mcpSetServerEnabled = (name: string, enabled: boolean) =>
  invoke<McpPanelState>("mcp_set_server_enabled", { name, enabled });
export const listPacks = () => invoke<PackManifest[]>("list_packs");
export const petList = () => invoke<InstalledPet[]>("pet_list");
export const petImport = (fileName: string, bytes: number[]) =>
  invoke<InstalledPet>("pet_import", { fileName, bytes });
export const petRemove = (id: string) => invoke<void>("pet_remove", { id });
export const petResolve = (id: string) => invoke<ResolvedPet>("pet_resolve", { id });
export const importPackZip = (fileName: string, bytes: number[]) =>
  invoke<PackImportResult>("import_pack_zip", { fileName, bytes });
export const readPackManifestJson = (id: string) => invoke<string>("read_pack_manifest_json", { id });
export const savePackManifestJson = (id: string, rawJson: string) =>
  invoke<PackManifest>("save_pack_manifest_json", { id, rawJson });
export const previewPackLorebook = (id: string, query: string) =>
  invoke<string>("preview_pack_lorebook", { id, query });
export const importPackLive2dFolder = (packId: string, srcDir: string) =>
  invoke<PackManifest>("import_pack_live2d_folder", { packId, srcDir });
export const resolvePackLive2dPath = (packId: string) =>
  invoke<string>("resolve_pack_live2d_path", { packId });
export const packLive2dBundle = (packId: string) =>
  invoke<Live2DBundle>("pack_live2d_bundle", { packId });
export const removePackLive2d = (packId: string) =>
  invoke<PackManifest>("remove_pack_live2d", { packId });
export const openPackDir = (id: string) => invoke<void>("open_pack_dir", { id });
export const importPackLoreFiles = (id: string, files: PackLoreFile[]) =>
  invoke<PackManifest>("import_pack_lore_files", { id, files });
export const listPackFiles = (id: string, subDir?: string) =>
  invoke<PackFileEntry[]>("list_pack_files", { id, subDir: subDir ?? null });
export const readPackFile = (id: string, path: string) =>
  invoke<PackFileContent>("read_pack_file", { id, path });
export const lorebookIndexStatus = (id: string) =>
  invoke<LoreIndexStatus>("lorebook_index_status", { id });
export const lorebookRecallDetail = (id: string, query: string, limit?: number) =>
  invoke<LoreRecallDetail>("lorebook_recall_detail", { id, query, limit: limit ?? null });
export const lorebookRebuildIndex = (id: string) =>
  invoke<LoreIndexStatus>("lorebook_rebuild_index", { id });
export const embeddingProbe = (settings: Settings) =>
  invoke<EmbeddingProbeResult>("embedding_probe", { settings });
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
export const mainWindowMinimize = () => invoke<void>("main_window_minimize");
export const mainWindowToggleMaximize = () => invoke<void>("main_window_toggle_maximize");
export const mainWindowClose = () => invoke<void>("main_window_close");
export const desktopCompanionRestore = () => invoke<void>("desktop_companion_restore");
export const desktopCompanionShowMain = () => invoke<void>("desktop_companion_show_main");
export const openWidgetsWindow = () => invoke<void>("open_widgets_window");
export const openLive2dWindow = () => invoke<void>("open_live2d_window");
export const pomodoroState = () => invoke<PomodoroPanelState>("pomodoro_state");
export const pomodoroStart = (request: PomodoroStartRequest) =>
  invoke<PomodoroPanelState>("pomodoro_start", { request });
export const pomodoroPause = () => invoke<PomodoroPanelState>("pomodoro_pause");
export const pomodoroResume = () => invoke<PomodoroPanelState>("pomodoro_resume");
export const pomodoroSkip = (request?: PomodoroSkipRequest) =>
  invoke<PomodoroPanelState>("pomodoro_skip", { request: request ?? null });
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
export const integrationScan = () => invoke<IntegrationSnapshot>("integration_scan");
export const integrationImportSkill = (sourcePath: string, source?: string) =>
  invoke<SkillCandidate>("integration_import_skill", { sourcePath, source: source ?? null });
export const integrationSetSkillEnabled = (id: string, enabled: boolean) =>
  invoke<void>("integration_set_skill_enabled", { id, enabled });
export const integrationRemoveSkill = (id: string) => invoke<void>("integration_remove_skill", { id });
export const integrationSessionMessages = (provider: string, sourcePath: string) =>
  invoke<ExternalSessionMessage[]>("integration_session_messages", { provider, sourcePath });
export const integrationImportSession = (provider: string, sourcePath: string) =>
  invoke<ImportedSession>("integration_import_session", { provider, sourcePath });
export const integrationImportConfig = (provider: string, kind: string) =>
  invoke<ImportedConfig>("integration_import_config", { provider, kind });
export const integrationMarketSearch = (query: string, limit = 20) =>
  invoke<MarketSearchResult>("integration_market_search", { query, limit });
export const integrationMarketInstall = (skill: MarketSkill) =>
  invoke<SkillCandidate>("integration_market_install", { skill });
export const usageSummary = (startAt?: number, endAt?: number) =>
  invoke<UsageSummary>("usage_summary", { startAt: startAt ?? null, endAt: endAt ?? null });
export const modelCatalog = () => invoke<ModelCatalog>("model_catalog");
export const refreshModelCatalog = () => invoke<ModelCatalog>("model_catalog_refresh");
export const memoryPanelState = () => invoke<MemoryPanelState>("memory_panel_state");
export const memoryAddEntry = (scope: string, kind: string, text: string) =>
  invoke<MemoryPanelState>("memory_add_entry", { scope, kind, text });
export const memoryUpdateEntry = (id: string, kind: string, text: string) =>
  invoke<MemoryPanelState>("memory_update_entry", { id, kind, text });
export const memoryDeleteEntry = (id: string) => invoke<MemoryPanelState>("memory_delete_entry", { id });
export const memoryDedupeApply = () => invoke<MemoryPanelState>("memory_dedupe_apply");
export const memoryMigrateNamespace = (fromNs: string, toNs: string) =>
  invoke<MemoryPanelState>("memory_migrate_namespace", { fromNs, toNs });
export const openSandbox = () => invoke<void>("open_sandbox");
export const workspaceState = () => invoke<WorkspaceState>("workspace_state");
export const selectWorkspace = (path: string) =>
  invoke<WorkspaceState>("select_workspace", { path });
export const listWorkspaceDirectory = (relativePath?: string) =>
  invoke<WorkspaceEntry[]>("list_workspace_directory", { relativePath: relativePath ?? null });
export const readWorkspaceFile = (relativePath: string) =>
  invoke<WorkspaceFilePreview>("read_workspace_file", { relativePath });
export const gitBranches = (expectedWorkspacePath: string) =>
  invoke<GitBranch[]>("git_branches", { expectedWorkspacePath });
export const switchGitBranch = (branch: string, expectedWorkspacePath: string) =>
  invoke<WorkspaceState>("switch_git_branch", { branch, expectedWorkspacePath });
export const gitChangedFiles = () => invoke<GitChangedFile[]>("git_changed_files");
export const webdavCheckConnection = (config: WebDavConfig) =>
  invoke<string>("webdav_check_connection", { config });
export const webdavBackupNow = (config: WebDavConfig) => invoke<string>("webdav_backup_now", { config });
export const webdavListBackups = (config: WebDavConfig) =>
  invoke<WebDavBackupFile[]>("webdav_list_backups", { config });
export const webdavDeleteBackup = (config: WebDavConfig, fileName: string) =>
  invoke<void>("webdav_delete_backup", { config, fileName });

// 会话管理
export const listSessions = () => invoke<SessionList>("list_sessions");
export const navigationSnapshot = (expectedSessionId?: string) =>
  invoke<NavigationSnapshot>("navigation_snapshot", { expectedSessionId: expectedSessionId ?? null });
export const sessionStats = (offset: number) => invoke<StatsPanel>("session_stats", { offset });
export const newSession = () => invoke<NavigationSnapshot>("new_session");
export const selectSession = (id: string) => invoke<NavigationSnapshot>("select_session", { id });
export const deleteSession = (id: string) => invoke<NavigationSnapshot>("delete_session", { id });
export const renameSession = (id: string, title: string) => invoke<string>("rename_session", { id, title });
export const setSessionArchived = (id: string, archived: boolean) =>
  invoke<void>("set_session_archived", { id, archived });

// Voice APIs. STT uses the configured recording backend; one-shot TTS can
// route through the DashScope media adapter when voice_tts_backend=dashscope.
export const voiceStatus = () => invoke<VoiceStatus>("voice_status");
export const voiceTranscribe = (audio: number[], mimeType?: string, language?: string) =>
  invoke<string>("voice_transcribe", { audio, mimeType, language });
export interface VoiceSynthesizeOptions {
  speed?: number;
  emotion?: string;
  streaming?: boolean;
}

export const voiceSynthesize = (text: string, voiceId?: string, options: VoiceSynthesizeOptions = {}) =>
  invoke<string>("voice_synthesize", {
    text,
    voiceId,
    speed: options.speed ?? null,
    emotion: options.emotion ?? null,
    streaming: options.streaming ?? null,
  });
export const voiceTtsCheck = (settings: Settings) =>
  invoke<ConnectionTestResult>("voice_tts_check", { settings });

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
export const listenPetCatalogUpdated = (handler: (e: InstalledPet[]) => void) =>
  listen<InstalledPet[]>("pet-catalog-updated", (e) => handler(e.payload));
export const listenMcpUpdated = (handler: (e: McpPanelState) => void) =>
  listen<McpPanelState>("mcp-updated", (e) => handler(e.payload));
export const listenPomodoroUpdated = (handler: (e: PomodoroPanelState) => void) =>
  listen<PomodoroPanelState>("pomodoro-updated", (e) => handler(e.payload));
export const listenPomodoroCompleted = (handler: (e: PomodoroCompletedEvent) => void) =>
  listen<PomodoroCompletedEvent>("pomodoro-completed", (e) => handler(e.payload));
export const listenSessionEngineUpdated = (handler: (e: SessionEnginePanelState) => void) =>
  listen<SessionEnginePanelState>("session-engine-updated", (e) => handler(e.payload));
export const listenUnifiedAgentEvents = (handler: (e: AgentEventEnvelope) => void) =>
  listen<AgentEventEnvelope>("agent-event", (e) => handler(e.payload));

// ---- 事件订阅 ----
export interface AgentEventHandlers {
  onAssistantStart: (turn?: TurnEventContext) => void;
  onAssistantDelta: (text: string, turn?: TurnEventContext) => void;
  onAssistantReasoning?: (text: string, turn?: TurnEventContext) => void;
  onAssistantDone: (text: string, turn?: TurnEventContext) => void;
  onAssistantError: (e: AssistantErrorEvent, turn?: TurnEventContext) => void;
  onAssistantInterrupted: (turn?: TurnEventContext) => void;
  onToolStart: (e: ToolStartEvent, turn?: TurnEventContext) => void;
  onToolEnd: (e: ToolEndEvent, turn?: TurnEventContext) => void;
  onConfirmRequest: (e: ConfirmRequestEvent) => void;
  onGoalProgress: (e: GoalProgressEvent) => void;
}

/// Consume the session-owned unified envelope for turn events. Confirmation
/// and goal progress have their own session id because they are emitted by
/// subsystems outside the session-engine event adapter.
export async function listenAgentEvents(h: AgentEventHandlers): Promise<UnlistenFn> {
  const uns: UnlistenFn[] = await Promise.all([
    listenUnifiedAgentEvents((event) => {
      const turn = event.turn;
      switch (event.kind) {
        case "assistant_start":
          h.onAssistantStart(turn);
          break;
        case "assistant_delta":
          h.onAssistantDelta(String(event.payload ?? ""), turn);
          break;
        case "assistant_reasoning":
          h.onAssistantReasoning?.(String(event.payload ?? ""), turn);
          break;
        case "assistant_done":
          h.onAssistantDone(String(event.payload ?? ""), turn);
          break;
        case "assistant_error":
          h.onAssistantError(event.payload as AssistantErrorEvent, turn);
          break;
        case "assistant_interrupted":
          h.onAssistantInterrupted(turn);
          break;
        case "tool_start":
          h.onToolStart(event.payload as ToolStartEvent, turn);
          break;
        case "tool_end":
          h.onToolEnd(event.payload as ToolEndEvent, turn);
          break;
      }
    }),
    listen<ConfirmRequestEvent>("tool-confirm-request", (e) => h.onConfirmRequest(e.payload)),
    listen<GoalProgressEvent>("goal-progress", (e) => h.onGoalProgress(e.payload)),
  ]);
  return () => uns.forEach((u) => u());
}

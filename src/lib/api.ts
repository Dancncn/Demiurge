import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentPanelState,
  ConfirmRequestEvent,
  ContextPanelState,
  GoalProgressEvent,
  Message,
  MemoryPanelState,
  OcrDownloadProgress,
  OcrModelSource,
  OcrModelStatus,
  PackManifest,
  PermissionScope,
  PermissionPanelState,
  SessionList,
  Settings,
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
export const respondConfirm = (id: string, allow: boolean, scope: PermissionScope) =>
  invoke<void>("respond_confirm", { id, allow, scope });
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const permissionPanelState = () => invoke<PermissionPanelState>("permission_panel_state");
export const permissionResetRule = (scope: PermissionScope, tool: string) =>
  invoke<PermissionPanelState>("permission_reset_rule", { scope, tool });
export const listPacks = () => invoke<PackManifest[]>("list_packs");
export const agentPanelState = () => invoke<AgentPanelState>("agent_panel_state");
export const getHistory = () => invoke<Message[]>("get_history");
export const contextPanelState = () => invoke<ContextPanelState>("context_panel_state");
export const memoryPanelState = () => invoke<MemoryPanelState>("memory_panel_state");
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
export const newSession = () => invoke<string>("new_session");
export const selectSession = (id: string) => invoke<void>("select_session", { id });
export const deleteSession = (id: string) => invoke<string>("delete_session", { id });
export const renameSession = (id: string, title: string) => invoke<string>("rename_session", { id, title });

// Voice API placeholders. These commands intentionally return a clear
// "backend not implemented" error until a concrete STT/TTS provider is chosen.
export const voiceStatus = () => invoke<VoiceStatus>("voice_status");
export const voiceTranscribe = (audioPath: string) => invoke<string>("voice_transcribe", { audioPath });
export const voiceSynthesize = (text: string, voiceId?: string) =>
  invoke<string>("voice_synthesize", { text, voiceId });

export const ocrModelStatus = () => invoke<OcrModelStatus>("ocr_model_status");
export const ocrImageBytes = (bytes: number[]) => invoke<string>("ocr_image_bytes", { bytes });
export const ocrDownloadModels = (source: OcrModelSource) =>
  invoke<OcrModelStatus>("ocr_download_models", { source });
export const listenOcrDownloadProgress = (handler: (e: OcrDownloadProgress) => void) =>
  listen<OcrDownloadProgress>("ocr-download-progress", (e) => handler(e.payload));

export const workflowPanelState = () => invoke<WorkflowPanelState>("workflow_panel_state");
export const workflowRun = (name: string) => invoke<string>("workflow_run", { name });
export const workflowStop = (runId: string) => invoke<void>("workflow_stop", { runId });
export const listenWorkflowUpdated = (handler: (e: WorkflowPanelState) => void) =>
  listen<WorkflowPanelState>("workflow-updated", (e) => handler(e.payload));

// ---- 事件订阅 ----
export interface AgentEventHandlers {
  onAssistantStart: () => void;
  onAssistantDelta: (text: string) => void;
  onAssistantDone: (text: string) => void;
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
    listen<string>("assistant-done", (e) => h.onAssistantDone(e.payload)),
    listen("assistant-interrupted", () => h.onAssistantInterrupted()),
    listen<ToolStartEvent>("tool-start", (e) => h.onToolStart(e.payload)),
    listen<ToolEndEvent>("tool-end", (e) => h.onToolEnd(e.payload)),
    listen<ConfirmRequestEvent>("tool-confirm-request", (e) => h.onConfirmRequest(e.payload)),
    listen<GoalProgressEvent>("goal-progress", (e) => h.onGoalProgress(e.payload)),
  ]);
  return () => uns.forEach((u) => u());
}

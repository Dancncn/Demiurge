import { mockIPC, mockWindows, mockConvertFileSrc } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { Settings, Message, TurnRunState, PermissionPanelState, ShellPolicyState } from "../../src/lib/types";
import { setAutoPromptEnabled } from "../../src/lib/fortune";

const settings: Settings = {
  provider: "deepseek",
  permission_mode: "default",
  base_url: "https://api.deepseek.com/v1",
  api_key: "",
  model: "deepseek-chat",
  reasoning_effort: "auto",
  model_routing: {
    enabled: false,
    subagent_tier: "haiku",
    docs_agent_tier: "sonnet",
    haiku_model: "",
    sonnet_model: "",
    opus_model: "",
    fallback_models: [],
    auto_failover: true,
    max_failover_attempts: 2,
    failure_threshold: 2,
    cooldown_seconds: 60,
  },
  current_pack: "default",
  current_pet: "",
  max_context_chars: 512000,
  max_input_tokens: 128000,
  reserved_output_tokens: 16000,
  context_budget_auto: true,
  language: "en",
  theme: "system",
  appearance: "material_bloom",
  launch_on_startup: false,
  auto_memory_enabled: true,
  embedding_enabled: false,
  embedding_provider: "none",
  embedding_base_url: "",
  embedding_api_key: "",
  embedding_model: "",
  embedding_dims: 1024,
  hybrid_weight: 0.5,
  companion_enabled: false,
  companion_memory_extraction_enabled: false,
  companion_memory_extraction_scope: "recent_turn",
  companion_tone: "gentle",
  companion_mood: "neutral",
  companion_energy: "normal",
  companion_focus: "available",
  companion_do_not_disturb: "",
  desktop_companion_enabled: false,
  desktop_companion_always_on_top: true,
  desktop_companion_click_through: false,
  desktop_companion_collapsed: false,
  weather_enabled: false,
  weather_location_mode: "manual",
  weather_city: "",
  weather_provider: "open_meteo",
  voice_enabled: true,
  voice_stt_backend: "",
  voice_tts_backend: "smoke",
  voice_id: "",
  voice_speed: 1,
  voice_emotion: "",
  voice_streaming: false,
  voice_tts_fallback: true,
  voice_hotkey_enabled: true,
  voice_hotkey: "Ctrl+Shift+Space",
  computer_use_enabled: false,
  ocr_model_source: "modelscope",
  web_search_provider: "auto",
  tavily_api_key: "",
  brave_search_api_key: "",
  exa_api_key: "",
  webdav_enabled: false,
  webdav_url: "",
  webdav_username: "",
  webdav_password: "",
  webdav_path: "",
  media_provider: "dashscope",
  media_base_url: "",
  media_api_key: "",
  image_model: "",
  image_size: "",
  tts_model: "",
  tts_voice: "",
  mcp_servers: [],
};

let activeSession = "session-a";
let runSequence = 0;
const sessions = [
  { id: "session-a", title: "Smoke A", updated_at: 2, archived: false },
  { id: "session-b", title: "Smoke B", updated_at: 1, archived: false },
];
const histories = new Map<string, Message[]>(sessions.map((session) => [session.id, []]));
const workspace = { path: "C:/smoke", name: "Smoke workspace", is_git: false, dirty: false };
let engine: { busy: boolean; cancel_requested: boolean; active_turn?: TurnRunState; last_turn?: TurnRunState } = {
  busy: false, cancel_requested: false,
};
let pending: { turn: TurnRunState; resolve: () => void; reject: (error: Error) => void } | undefined;
let lastRun: TurnRunState | undefined;
const synthesis: Array<(url: string) => void> = [];
const unknownCommands: string[] = [];
const calls: string[] = [];
let permissionRefreshFails = false;
const permissions: PermissionPanelState = {
  rules: [{ tool: "shell", effect: "ask", scope: "user", reason: "Smoke saved rule", updated_at: 1 }],
  audit: [], notices: [],
  tools: [{ tool: "shell", description: "Run a command", risk: "privileged", default_effect: "ask", default_scope: "once", default_reason: "Ask before commands" }],
};
const shellPolicy: ShellPolicyState = {
  platform: "windows", default_isolation: "standard", strict_timeout_secs: 15, max_timeout_secs: 120,
  env_allowlist: ["PATH"], strict_blocked_risks: [], risk_rules: [],
  containment: { process_group: true, kill_process_tree_on_timeout: true, filesystem_sandbox: "partial", network_sandbox: "partial", notes: [] },
};
const playedAudio: HTMLMediaElement[] = [];
const originalPlay = HTMLMediaElement.prototype.play;
HTMLMediaElement.prototype.play = function () {
  playedAudio.push(this);
  return originalPlay.call(this);
};

function snapshot() {
  return structuredClone({ session_id: activeSession, sessions, workspace, goal: null, history: histories.get(activeSession) });
}

async function agentEvent(kind: string, payload: unknown, responseId: string, turn = lastRun) {
  if (!turn) throw new Error("Send through the real Composer before emitting events");
  await emit("agent-event", { kind, payload, response_id: responseId, turn, timestamp: Date.now() });
}

async function settle(error?: string) {
  if (!pending) throw new Error("No pending send");
  const command = pending;
  pending = undefined;
  lastRun = { ...command.turn, status: error ? "failed" : engine.cancel_requested ? "interrupted" : "completed" };
  engine = { busy: false, cancel_requested: false, last_turn: lastRun };
  await emit("session-engine-updated", engine);
  if (error) command.reject(new Error(error));
  else command.resolve();
}

// A real, silent 30-second WAV lets the browser exercise Audio.play/pause.
function silentAudioUrl() {
  const samples = 8000 * 30;
  const buffer = new ArrayBuffer(44 + samples * 2);
  const view = new DataView(buffer);
  const chars = (offset: number, text: string) => [...text].forEach((char, i) => view.setUint8(offset + i, char.charCodeAt(0)));
  chars(0, "RIFF"); view.setUint32(4, buffer.byteLength - 8, true); chars(8, "WAVEfmt ");
  view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true);
  view.setUint32(24, 8000, true); view.setUint32(28, 16000, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true);
  chars(36, "data"); view.setUint32(40, samples * 2, true);
  return URL.createObjectURL(new Blob([buffer], { type: "audio/wav" }));
}

setAutoPromptEnabled(false);
localStorage.setItem("demiurge.lang", "en");
mockWindows("main");
mockConvertFileSrc();
mockIPC(async (command, rawArgs) => {
  calls.push(command);
  const args = rawArgs as Record<string, unknown>;
  switch (command) {
    case "get_settings": return structuredClone(settings);
    case "list_packs": return [];
    case "agent_panel_state": return { definitions: [], agents_dir: "C:/smoke/agents" };
    case "navigation_snapshot": return snapshot();
    case "list_sessions": return { active: activeSession, sessions };
    case "select_session": activeSession = String(args.id); return snapshot();
    case "plan_state": return { active: false, approved: false };
    case "session_engine_state": return structuredClone(engine);
    case "context_panel_state": return null;
    case "read_pack_manifest_json": return "{}";
    case "ocr_model_status": return null;
    case "memory_panel_state": return null;
    case "companion_memory_suggestions": return [];
    case "companion_memory_queue_state": return null;
    case "companion_panel_state": return null;
    case "mcp_panel_state": return { servers: [], tools: [], resources: {} };
    case "model_catalog": return { models: [], fetched_at: 0 };
    case "permission_panel_state": {
      if (permissionRefreshFails) throw new Error("Smoke permission refresh failed");
      return structuredClone(permissions);
    }
    case "shell_policy_state": return structuredClone(shellPolicy);
    case "session_stats": return null;
    case "voice_status": return {
      enabled: true, stt_backend: "smoke", tts_backend: "smoke", voice_id: "", ready: true,
      reason: "", tts_ready: true, tts_reason: "", speed: 1, emotion: "", streaming: false, fallback_enabled: false,
    };
    case "voice_synthesize": return new Promise<string>((resolve) => synthesis.push(resolve));
    case "send":
    case "send_with_agents": {
      const turn: TurnRunState = {
        id: `smoke-turn-${++runSequence}`, session_id: activeSession, entrypoint: "send", status: "running",
        input_preview: String(args.text), agent_names: [], started_at: Date.now(), updated_at: Date.now(),
      };
      lastRun = turn;
      histories.get(activeSession)!.push({ role: "user", content: String(args.text) });
      engine = { busy: true, cancel_requested: false, active_turn: turn };
      const result = new Promise<void>((resolve, reject) => { pending = { turn, resolve, reject }; });
      await emit("session-engine-updated", engine);
      return result;
    }
    case "interrupt": {
      if (lastRun) {
        engine = { ...engine, cancel_requested: true, active_turn: { ...lastRun, status: "cancelling" } };
        await emit("session-engine-updated", engine);
        await agentEvent("assistant_interrupted", null, "stop-answer");
        await settle();
      }
      return;
    }
    case "plugin:window|is_maximized": return false;
    case "plugin:window|inner_size": return { width: 1440, height: 1000 };
    case "plugin:window|scale_factor": return 1;
    case "plugin:window|set_size": return;
    case "plugin:window|center": return;
    default:
      unknownCommands.push(command);
      throw new Error(`Unexpected smoke IPC: ${command}`);
  }
}, { shouldMockEvents: true });

Object.assign(window, { __DEMIURGE_SMOKE__: {
  pending: () => Boolean(pending),
  turn: () => structuredClone(lastRun),
  agentEvent,
  confirm(turn: TurnRunState, id: string) {
    return emit("tool-confirm-request", {
      id, session_id: turn.session_id, turn_id: turn.id, tool: "write_file", args: "{}", summary: "Smoke confirmation",
    });
  },
  settle,
  saveAnswer(text: string) { histories.get(lastRun!.session_id)!.push({ role: "assistant", content: text }); },
  calls: () => [...calls],
  unknownCommands: () => [...unknownCommands],
  failPermissionRefresh() { permissionRefreshFails = true; },
  synthesisCount: () => synthesis.length,
  releaseSynthesis() { for (const resolve of synthesis.splice(0)) resolve(silentAudioUrl()); },
  audio: () => ({ played: playedAudio.length, allPaused: playedAudio.every((audio) => audio.paused) }),
} });

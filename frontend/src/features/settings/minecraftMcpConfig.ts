import type { McpEnvVar, McpServerConfig } from "@/lib/types";

export const MINECRAFT_MCP_SERVER_NAME = "minecraft_ai";

export interface MinecraftMcpSettings {
  enabled: boolean;
  nodeCommand: string;
  bridgeScript: string;
  host: string;
  port: string;
  username: string;
  auth: "offline" | "microsoft";
  version: string;
  owner: string;
  dataDir: string;
  reconnectDelayMs: string;
  maxActionDistance: string;
  allowPvp: boolean;
  allowDropItems: boolean;
  autoResume: boolean;
  visionEnabled: boolean;
  visionFrameUrl: string;
  visionIntervalMs: string;
  residualThreshold: string;
  minRegionPixels: string;
  vlmEnabled: boolean;
}

const defaults: MinecraftMcpSettings = {
  enabled: false,
  nodeCommand: "node",
  bridgeScript: "",
  host: "127.0.0.1",
  port: "25565",
  username: "AI_Player",
  auth: "offline",
  version: "",
  owner: "FuQiang",
  dataDir: "",
  reconnectDelayMs: "5000",
  maxActionDistance: "128",
  allowPvp: false,
  allowDropItems: true,
  autoResume: true,
  visionEnabled: false,
  visionFrameUrl: "",
  visionIntervalMs: "1000",
  residualThreshold: "32",
  minRegionPixels: "64",
  vlmEnabled: false,
};

const managedEnvKeys = new Set([
  "MCP_STDIO",
  "NODE_ENV",
  "HEALTH_PORT",
  "LLM_MODE",
  "MC_HOST",
  "MC_PORT",
  "MC_USERNAME",
  "MC_AUTH",
  "MC_VERSION",
  "MC_OWNER",
  "MC_COMMAND_PREFIX",
  "MC_RECONNECT_DELAY_MS",
  "DATA_DIR",
  "MAX_ACTION_DISTANCE",
  "ALLOW_PVP",
  "ALLOW_DROP_ITEMS",
  "AUTO_RESUME",
  "VISION_ENABLED",
  "VISION_FRAME_URL",
  "VISION_INTERVAL_MS",
  "VISION_RESIDUAL_THRESHOLD",
  "VISION_MIN_REGION_PIXELS",
  "VLM_ENABLED",
]);

function envMap(server: McpServerConfig | undefined) {
  return new Map(server?.env.map((entry) => [entry.key, entry.value]) ?? []);
}

function bool(value: string | undefined, fallback: boolean) {
  if (value === undefined) return fallback;
  return value.toLowerCase() === "true";
}

export function findMinecraftMcpServer(servers: McpServerConfig[]) {
  return servers.find((server) => server.name === MINECRAFT_MCP_SERVER_NAME);
}

export function readMinecraftMcpSettings(server?: McpServerConfig): MinecraftMcpSettings {
  const env = envMap(server);
  return {
    ...defaults,
    enabled: server?.enabled ?? defaults.enabled,
    nodeCommand: server?.command || defaults.nodeCommand,
    bridgeScript: server?.args[0] ?? "",
    host: env.get("MC_HOST") || defaults.host,
    port: env.get("MC_PORT") || defaults.port,
    username: env.get("MC_USERNAME") || defaults.username,
    auth: env.get("MC_AUTH") === "microsoft" ? "microsoft" : "offline",
    version: env.get("MC_VERSION") ?? "",
    owner: env.get("MC_OWNER") || defaults.owner,
    dataDir: env.get("DATA_DIR") ?? "",
    reconnectDelayMs: env.get("MC_RECONNECT_DELAY_MS") || defaults.reconnectDelayMs,
    maxActionDistance: env.get("MAX_ACTION_DISTANCE") || defaults.maxActionDistance,
    allowPvp: bool(env.get("ALLOW_PVP"), defaults.allowPvp),
    allowDropItems: bool(env.get("ALLOW_DROP_ITEMS"), defaults.allowDropItems),
    autoResume: bool(env.get("AUTO_RESUME"), defaults.autoResume),
    visionEnabled: bool(env.get("VISION_ENABLED"), defaults.visionEnabled),
    visionFrameUrl: env.get("VISION_FRAME_URL") ?? "",
    visionIntervalMs: env.get("VISION_INTERVAL_MS") || defaults.visionIntervalMs,
    residualThreshold: env.get("VISION_RESIDUAL_THRESHOLD") || defaults.residualThreshold,
    minRegionPixels: env.get("VISION_MIN_REGION_PIXELS") || defaults.minRegionPixels,
    vlmEnabled: bool(env.get("VLM_ENABLED"), defaults.vlmEnabled),
  };
}

function managedEnv(settings: MinecraftMcpSettings): McpEnvVar[] {
  const values: Record<string, string> = {
    MCP_STDIO: "true",
    NODE_ENV: "production",
    HEALTH_PORT: "0",
    LLM_MODE: "mock",
    MC_HOST: settings.host,
    MC_PORT: settings.port,
    MC_USERNAME: settings.username,
    MC_AUTH: settings.auth,
    MC_VERSION: settings.version,
    MC_OWNER: settings.owner,
    MC_RECONNECT_DELAY_MS: settings.reconnectDelayMs,
    DATA_DIR: settings.dataDir,
    MAX_ACTION_DISTANCE: settings.maxActionDistance,
    ALLOW_PVP: String(settings.allowPvp),
    ALLOW_DROP_ITEMS: String(settings.allowDropItems),
    AUTO_RESUME: String(settings.autoResume),
    VISION_ENABLED: String(settings.visionEnabled),
    VISION_FRAME_URL: settings.visionFrameUrl,
    VISION_INTERVAL_MS: settings.visionIntervalMs,
    VISION_RESIDUAL_THRESHOLD: settings.residualThreshold,
    VISION_MIN_REGION_PIXELS: settings.minRegionPixels,
    VLM_ENABLED: String(settings.vlmEnabled),
  };
  return Object.entries(values).map(([key, value]) => ({ key, value, secret: false }));
}

export function writeMinecraftMcpSettings(
  previous: McpServerConfig | undefined,
  settings: MinecraftMcpSettings,
): McpServerConfig {
  const extraEnv = previous?.env.filter((entry) => !managedEnvKeys.has(entry.key)) ?? [];
  return {
    name: MINECRAFT_MCP_SERVER_NAME,
    enabled: settings.enabled,
    transport: "stdio",
    command: settings.nodeCommand.trim(),
    args: settings.bridgeScript.trim() ? [settings.bridgeScript.trim()] : [],
    env: [...managedEnv(settings), ...extraEnv],
  };
}

export function upsertMinecraftMcpServer(
  servers: McpServerConfig[],
  settings: MinecraftMcpSettings,
) {
  const previous = findMinecraftMcpServer(servers);
  const next = writeMinecraftMcpSettings(previous, settings);
  if (!previous) return [...servers, next];
  return servers.map((server) => (server.name === MINECRAFT_MCP_SERVER_NAME ? next : server));
}

export function removeMinecraftMcpServer(servers: McpServerConfig[]) {
  return servers.filter((server) => server.name !== MINECRAFT_MCP_SERVER_NAME);
}

export function minecraftProjectPaths(root: string) {
  const separator = root.includes("\\") ? "\\" : "/";
  const base = root.replace(/[\\/]+$/, "");
  return {
    bridgeScript: [base, "dist", "mcp", "index.js"].join(separator),
    dataDir: [base, "data", "demiurge"].join(separator),
  };
}

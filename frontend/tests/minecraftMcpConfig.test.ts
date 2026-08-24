import assert from "node:assert/strict";
import test from "node:test";
import {
  MINECRAFT_MCP_SERVER_NAME,
  minecraftProjectPaths,
  readMinecraftMcpSettings,
  upsertMinecraftMcpServer,
} from "../src/features/settings/minecraftMcpConfig.ts";

test("minecraft settings persist as an independent MCP server without a second LLM credential", () => {
  const settings = readMinecraftMcpSettings();
  const servers = upsertMinecraftMcpServer([], {
    ...settings,
    nodeCommand: "C:\\runtime\\node.exe",
    bridgeScript: "C:\\projects\\minecraft-ai\\dist\\mcp\\index.js",
    dataDir: "C:\\projects\\minecraft-ai\\data\\demiurge",
    host: "mc.example.test",
    username: "DemiurgeBot",
  });

  assert.equal(servers.length, 1);
  assert.equal(servers[0]?.name, MINECRAFT_MCP_SERVER_NAME);
  assert.equal(servers[0]?.command, "C:\\runtime\\node.exe");
  assert.deepEqual(servers[0]?.args, ["C:\\projects\\minecraft-ai\\dist\\mcp\\index.js"]);
  const env = new Map(servers[0]?.env.map((entry) => [entry.key, entry.value]));
  assert.equal(env.get("MCP_STDIO"), "true");
  assert.equal(env.get("LLM_MODE"), "mock");
  assert.equal(env.get("MC_HOST"), "mc.example.test");
  assert.equal(env.get("MC_USERNAME"), "DemiurgeBot");
  assert.equal(env.get("MC_OWNER"), "FuQiang");
  assert.equal(env.has("MC_COMMAND_PREFIX"), false);
  assert.equal(env.has("LLM_API_KEY"), false);
  assert.equal(env.has("LLM_BASE_URL"), false);
  assert.equal(env.has("LLM_MODEL"), false);
  assert.equal(env.has("VLM_MODEL"), false);
});

test("choosing a project root derives the built bridge and isolated data paths", () => {
  assert.deepEqual(minecraftProjectPaths("C:\\projects\\minecraft-ai\\"), {
    bridgeScript: "C:\\projects\\minecraft-ai\\dist\\mcp\\index.js",
    dataDir: "C:\\projects\\minecraft-ai\\data\\demiurge",
  });
  assert.deepEqual(minecraftProjectPaths("/opt/minecraft-ai/"), {
    bridgeScript: "/opt/minecraft-ai/dist/mcp/index.js",
    dataDir: "/opt/minecraft-ai/data/demiurge",
  });
});

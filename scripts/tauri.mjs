#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import net from "node:net";
import path from "node:path";
import process from "node:process";

const PREFERRED_DEV_PORT = 38741;
const EPHEMERAL_MIN = 49152;
const EPHEMERAL_MAX = 65535;

const args = process.argv.slice(2);
const tauriCli = path.resolve("node_modules", "@tauri-apps", "cli", "tauri.js");

function parsePort(value) {
  if (!value) return undefined;
  const port = Number(value);
  return Number.isInteger(port) && port > 0 && port <= 65535 ? port : undefined;
}

function canListen(host, port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => {
      server.close(() => resolve(true));
    });
    server.listen({ host, port });
  });
}

async function chooseDevPort(host) {
  const preferred = parsePort(process.env.DEMIURGE_PREFERRED_DEV_PORT) ?? PREFERRED_DEV_PORT;
  if (await canListen(host, preferred)) return preferred;

  for (let attempt = 0; attempt < 100; attempt += 1) {
    const port = EPHEMERAL_MIN + Math.floor(Math.random() * (EPHEMERAL_MAX - EPHEMERAL_MIN + 1));
    if (await canListen(host, port)) return port;
  }

  for (let port = EPHEMERAL_MIN; port <= EPHEMERAL_MAX; port += 1) {
    if (await canListen(host, port)) return port;
  }

  throw new Error("No available dev server port found in 49152-65535.");
}

function runTauri(nextArgs, env = process.env) {
  const child = spawn(process.execPath, [tauriCli, ...nextArgs], {
    stdio: "inherit",
    env,
    shell: false,
  });
  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

async function runDev() {
  const passthrough = args.slice(1);
  const host = process.env.TAURI_DEV_HOST || "127.0.0.1";
  const port = await chooseDevPort(host);
  const devUrl = `http://${host}:${port}`;
  const generatedDir = path.resolve(".tauri-dev");
  const generatedConfig = path.join(generatedDir, "tauri.dev.conf.json");

  mkdirSync(generatedDir, { recursive: true });
  writeFileSync(
    generatedConfig,
    JSON.stringify(
      {
        build: {
          devUrl,
          beforeDevCommand: "npm run dev",
        },
      },
      null,
      2,
    ),
  );

  console.log(`[demiurge] dev server: ${devUrl}`);
  console.log(`[demiurge] tauri config override: ${generatedConfig}`);

  if (process.env.DEMIURGE_TAURI_DRY_RUN === "1") return;

  runTauri(["dev", "--config", generatedConfig, ...passthrough], {
    ...process.env,
    DEMIURGE_DEV_HOST: host,
    DEMIURGE_DEV_PORT: String(port),
  });
}

if (args[0] === "dev") {
  runDev().catch((error) => {
    console.error(`[demiurge] ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
} else {
  runTauri(args);
}

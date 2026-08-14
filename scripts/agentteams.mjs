import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const manifest = resolve(root, "competition", "agentteams", "agentteams-goai.yaml");
const action = process.argv[2] ?? "check";

if (!existsSync(manifest)) {
  console.error(`AgentTeams manifest not found: ${manifest}`);
  process.exit(1);
}

const source = readFileSync(manifest, "utf8");
const kinds = [...source.matchAll(/^kind:\s*(\w+)\s*$/gm)].map((match) => match[1]);
const expected = ["Manager", "Worker", "Worker", "Worker", "Worker", "Team", "Human"];

if (JSON.stringify(kinds) !== JSON.stringify(expected)) {
  console.error(`Unexpected AgentTeams resource order. Expected ${expected.join(" -> ")}, got ${kinds.join(" -> ")}`);
  process.exit(1);
}

const skillNames = [...source.matchAll(/^\s+- (goai-[a-z0-9-]+)\s*$/gm)].map((match) => match[1]);
const missingSkills = [...new Set(skillNames)].filter(
  (name) => !existsSync(resolve(root, "packs", "default", "skills", name, "SKILL.md")),
);
if (missingSkills.length > 0) {
  console.error(`AgentTeams manifest references missing local Skills: ${missingSkills.join(", ")}`);
  process.exit(1);
}

if (action === "check") {
  const placeholders = (source.match(/replace-with-higress-host/g) ?? []).length;
  console.log(`AgentTeams manifest OK: ${kinds.length} resources in dependency order.`);
  console.log(`Local GOAI Skills mapped: ${new Set(skillNames).size}.`);
  console.log(`Higress endpoint placeholders: ${placeholders}; replace them before apply.`);
  process.exit(0);
}

if (action !== "apply") {
  console.error("Usage: node scripts/agentteams.mjs check|apply");
  process.exit(1);
}

if (source.includes("replace-with-higress-host")) {
  console.error("Refusing to apply: replace the Higress endpoint placeholder first.");
  process.exit(1);
}

const result = spawnSync("agt", ["apply", "-f", manifest], { stdio: "inherit", shell: process.platform === "win32" });
process.exit(result.status ?? 1);

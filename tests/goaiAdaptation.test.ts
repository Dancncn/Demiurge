import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, resolve } from "node:path";
import test from "node:test";

const repoRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));

test("GOAI AgentTeams manifest declares the official collaboration topology", () => {
  const manifest = readFileSync(
    join(repoRoot, "competition", "agentteams", "agentteams-goai.yaml"),
    "utf8",
  );
  const kinds = [...manifest.matchAll(/^kind:\s*(\w+)\s*$/gm)].map((match) => match[1]);

  assert.deepEqual(kinds, ["Manager", "Worker", "Worker", "Worker", "Worker", "Team", "Human"]);
  assert.match(manifest, /apiVersion:\s*agentteams\.io\/v1beta1/);
  assert.match(manifest, /role:\s*team_leader/);
  assert.match(manifest, /role:\s*worker/);
  assert.match(manifest, /accessibleTeams:/);
});

test("GOAI Skills expose the contest-required reusable capability contract", () => {
  const skillsRoot = join(repoRoot, "packs", "default", "skills");
  const skillDirs = readdirSync(skillsRoot)
    .filter((name) => name.startsWith("goai-"))
    .sort();

  assert.ok(skillDirs.length >= 8, `expected at least 8 GOAI Skills, found ${skillDirs.length}`);
  for (const requiredSkill of [
    "goai-data-processing-expert",
    "goai-compliance-process",
    "goai-report-agent",
  ]) {
    assert.ok(skillDirs.includes(requiredSkill), `missing business Skill: ${requiredSkill}`);
  }

  for (const skillDir of skillDirs) {
    const skill = readFileSync(join(skillsRoot, skillDir, "SKILL.md"), "utf8");
    assert.match(skill, /^---[\s\S]*?name:/m, skillDir);
    assert.match(skill, /description:/, skillDir);
    assert.match(skill, /triggers:/, skillDir);
    assert.match(skill, /tools:/, skillDir);
    assert.match(skill, /required_permissions:/, skillDir);
    for (const heading of [
      "Input and output",
      "Call conditions",
      "Dependencies",
      "Failure handling",
      "Security boundary",
      "Reuse value and handoff",
    ]) {
      assert.match(skill, new RegExp(`## ${heading}`), `${skillDir}: ${heading}`);
    }
  }

  const businessToolContracts = {
    "goai-data-processing-expert": "mcp__ecu_lab__excel_build_raw_data",
    "goai-compliance-process": "mcp__ecu_lab__trial_parse_outline",
    "goai-report-agent": "mcp__ecu_lab__trial_build_report",
  } as const;
  for (const [skillDir, tool] of Object.entries(businessToolContracts)) {
    const skill = readFileSync(join(skillsRoot, skillDir, "SKILL.md"), "utf8");
    assert.match(skill, new RegExp(tool), `${skillDir}: business tool contract`);
  }

  const retiredBusinessIdentities = /(?:a2l-signal-catalog|mda-measurement-operator|ecu-report-verifier)/;
  for (const skillDir of [
    "goai-task-decomposer",
    "goai-mda-measurement-capture",
    "goai-excel-report-validator",
  ]) {
    const skill = readFileSync(join(skillsRoot, skillDir, "SKILL.md"), "utf8");
    assert.doesNotMatch(skill, retiredBusinessIdentities, `${skillDir}: retired business Identity reference`);
  }
});

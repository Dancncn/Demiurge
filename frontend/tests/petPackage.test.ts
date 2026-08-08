import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = async (path: string) =>
  (await readFile(new URL(`../${path}`, import.meta.url), "utf8")).replace(/\r\n/g, "\n");
const backendSource = async (path: string) =>
  (await readFile(new URL(`../../backend/Demiurge-desktop/${path}`, import.meta.url), "utf8")).replace(
    /\r\n/g,
    "\n",
  );

test("pet packages use an independent typed IPC flow", async () => {
  const [api, controller, biz, starter] = await Promise.all([
    source("src/lib/api.ts"),
    backendSource("src/controller/pet.rs"),
    backendSource("src/biz/pet.rs"),
    backendSource("src/starter.rs"),
  ]);

  for (const command of ["pet_list", "pet_import", "pet_remove", "pet_resolve"]) {
    assert.match(api, new RegExp(`invoke<[^>]+>\\(\"${command}\"`));
    assert.match(controller, new RegExp(`fn ${command}\\b`));
    assert.match(starter, new RegExp(`^\\s+${command},$`, "m"));
  }
  assert.match(controller, /crate::biz::pet::/);
  assert.doesNotMatch(controller, /std::fs|state\.pets_dir\.lock/);
  assert.match(biz, /crate::pet::/);
});

test("pet selection remains independent from character packs", async () => {
  const [types, settings, petPanel, settingsDialog] = await Promise.all([
    source("src/lib/types.ts"),
    backendSource("../Demiurge-common/src/settings.rs"),
    source("src/features/pet/PetSettingsPanel.tsx"),
    source("src/features/settings/SettingsDialog.tsx"),
  ]);

  assert.match(types, /current_pet: string/);
  assert.match(settings, /pub current_pet: String/);
  assert.match(petPanel, /currentPet/);
  assert.match(settingsDialog, /currentPet=\{form\.current_pet\}/);
  assert.doesNotMatch(petPanel, /current_pack/);
});

test("desktop companion shell renders the selected pet runtime", async () => {
  const [shell, canvas] = await Promise.all([
    source("src/features/companion/DesktopCompanionShell.tsx"),
    source("src/features/pet/PetCanvas.tsx"),
  ]);

  assert.match(shell, /PetCanvas/);
  assert.match(shell, /listenUnifiedAgentEvents/);
  assert.doesNotMatch(shell, /activePack\?\.avatarDataUrl/);
  assert.match(canvas, /onPointerEnter/);
  assert.match(canvas, /onPointerLeave/);
  assert.match(canvas, /audio\.loop/);
  assert.match(shell, /type: "motion\.started"/);
  assert.match(shell, /type: "motion\.ended"/);
  assert.match(shell, /await getCurrentWindow\(\)\.startDragging\(\)/);
  assert.match(shell, /\.onMoved\(/);
  assert.doesNotMatch(shell, /finally \{[\s\S]*?type: "motion\.ended"/);
  assert.doesNotMatch(shell, /data-tauri-drag-region/);
});

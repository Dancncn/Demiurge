// 下载 Live2D Cubism Core（live2dcubismcore.min.js）到 public/core/。
//
// Cubism Core 是 Live2D 官方的私有运行时（WASM 内嵌在该 JS 中），
// 许可证禁止第三方再分发，因此不随仓库分发，需用户自行下载。
// 非商业用途免费；商业用途请遵守 Live2D SDK Release License。
//
// 用法：npm run fetch:cubism-core
// 失败时打印手动下载指引。

import { mkdir, writeFile, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const DEST = join(ROOT, "public", "core", "live2dcubismcore.min.js");
const MIN_BYTES = 10 * 1024;

// 候选下载地址（按顺序尝试）。
const CANDIDATES = [
  "https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js",
  "https://cubism.live2d.com/sdk-web/bin/CubismCoreForWeb/live2dcubismcore.min.js",
];

async function alreadyPresent() {
  try {
    const s = await stat(DEST);
    return s.size > MIN_BYTES;
  } catch {
    return false;
  }
}

async function tryFetch(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.length <= MIN_BYTES) {
    throw new Error(`body too small (${buf.length} bytes)`);
  }
  return buf;
}

async function manualInstructions() {
  console.error("");
  console.error("自动下载失败。请手动下载 Cubism Core：");
  console.error("  1. 访问 https://www.live2d.com/en/sdk/download/web/");
  console.error("  2. 下载 Cubism SDK for Web（接受许可证）");
  console.error("  3. 解压 zip");
  console.error("  4. 将 Core/live2dcubismcore.min.js 复制到 public/core/live2dcubismcore.min.js");
  console.error("  5. 重新运行 npm run fetch:cubism-core 验证");
  process.exit(1);
}

async function main() {
  if (await alreadyPresent()) {
    console.log("Cubism Core 已存在，跳过下载。");
    return;
  }
  await mkdir(dirname(DEST), { recursive: true });

  let buf = null;
  let lastErr = null;
  for (const url of CANDIDATES) {
    try {
      console.log(`尝试下载：${url}`);
      buf = await tryFetch(url);
      console.log(`下载成功：${buf.length} 字节`);
      break;
    } catch (e) {
      lastErr = e;
      console.warn(`  失败：${e.message}`);
    }
  }
  if (!buf) {
    console.error(`所有候选地址均失败（最后错误：${lastErr?.message ?? "unknown"}）`);
    await manualInstructions();
    return;
  }

  await writeFile(DEST, buf);
  console.log(`已写入：${DEST}`);
}

main().catch((e) => {
  console.error("fetch-cubism-core 异常：", e);
  process.exit(1);
});

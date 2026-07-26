// Live2D engine singleton and asset URL bridge.
//
// All pixi.js / untitled-pixi-live2d-engine imports are dynamic so they only
// land in the vendor-live2d chunk when the user actually opens the panel.
//
// Cubism Core (live2dcubismcore.min.js) is a proprietary WASM runtime gated by
// the Live2D Proprietary Software License.  Users self-download it via
// `npm run fetch:cubism-core`; this module injects it as a <script> tag at
// runtime and throws a friendly error when it is missing.

import { convertFileSrc } from "@tauri-apps/api/core";
import type { Live2DBundle } from "./types";

// Module-level guards: Live2DPlugin is registered once, reloads reuse it.
let engineInitialized = false;
let coreLoading: Promise<void> | null = null;
let engineLoading: ReturnType<typeof loadEngineModules> | null = null;

declare global {
  interface Window {
    Live2DCubismCore?: unknown;
  }
}

export function ensureCubismCore(): Promise<void> {
  if (typeof window !== "undefined" && window.Live2DCubismCore) return Promise.resolve();
  if (coreLoading) return coreLoading;
  coreLoading = new Promise<void>((resolve, reject) => {
    const script = document.createElement("script");
    script.src = "/core/live2dcubismcore.min.js";
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => {
      coreLoading = null;
      reject(
        new Error(
          "Failed to load Cubism Core. Run `npm run fetch:cubism-core` to download live2dcubismcore.min.js into public/core/.",
        ),
      );
    };
    document.head.appendChild(script);
  });
  return coreLoading;
}

export interface Live2DLoadResult {
  app: Live2DPixiApp;
  model: Live2DModelLike;
}

export type Live2DLoadStage = "core" | "engine" | "renderer" | "model" | "ready";
export type Live2DProgressListener = (progress: number, stage: Live2DLoadStage) => void;

export interface Live2DBlobModelUrl {
  url: string;
  revoke(): void;
}

// ----------------------------------------------------------------- heap-free model-url builders

function normalizePackPath(path: string) {
  return path.replaceAll("\\", "/");
}

function base64ToBytes(data: string) {
  const binary = window.atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

type AssetResolver = (path: string, label: string) => string;

function rewritePath(value: unknown, resolveAsset: AssetResolver, label: string) {
  if (typeof value !== "string" || !value.trim()) return value;
  return resolveAsset(value, label);
}

function rewriteModelReferences(model: Record<string, unknown>, resolveAsset: AssetResolver) {
  const refs = model.FileReferences;
  if (!refs || typeof refs !== "object") return;
  const fileRefs = refs as Record<string, unknown>;

  for (const key of ["Moc", "Physics", "Pose", "DisplayInfo", "UserData"]) {
    if (key in fileRefs) fileRefs[key] = rewritePath(fileRefs[key], resolveAsset, key);
  }

  if (Array.isArray(fileRefs.Textures)) {
    fileRefs.Textures = fileRefs.Textures.map((path, index) =>
      rewritePath(path, resolveAsset, `Textures[${index}]`),
    );
  }

  if (Array.isArray(fileRefs.Expressions)) {
    for (const expression of fileRefs.Expressions) {
      if (expression && typeof expression === "object" && "File" in expression) {
        const entry = expression as Record<string, unknown>;
        entry.File = rewritePath(entry.File, resolveAsset, "Expressions.File");
      }
    }
  }

  if (fileRefs.Motions && typeof fileRefs.Motions === "object") {
    for (const motions of Object.values(fileRefs.Motions as Record<string, unknown>)) {
      if (!Array.isArray(motions)) continue;
      for (const motion of motions) {
        if (!motion || typeof motion !== "object") continue;
        const entry = motion as Record<string, unknown>;
        if ("File" in entry) entry.File = rewritePath(entry.File, resolveAsset, "Motions.File");
        if ("Sound" in entry) entry.Sound = rewritePath(entry.Sound, resolveAsset, "Motions.Sound");
      }
    }
  }
}

/** In-app (base64 bundle) path — kept for compatibility but the primary
  * rendering path now uses the direct asset URL via createLive2DAssetModelUrl. */
export function createLive2DBlobModelUrl(bundle: Live2DBundle): Live2DBlobModelUrl {
  const urls: string[] = [];
  const assetUrls = new Map<string, string>();

  for (const asset of bundle.assets) {
    const mime = asset.mime || "application/octet-stream";
    const url = mime.startsWith("image/")
      ? `data:${mime};base64,${asset.data}`
      : URL.createObjectURL(new Blob([base64ToBytes(asset.data)], { type: mime }));
    if (!mime.startsWith("image/")) urls.push(url);
    assetUrls.set(normalizePackPath(asset.path), url);
  }

  try {
    const model = JSON.parse(bundle.model_json) as Record<string, unknown>;
    rewriteModelReferences(model, (path, label) => {
      const url = assetUrls.get(normalizePackPath(path));
      if (!url) throw new Error(`Live2D bundle missing resource: ${label} -> ${path}`);
      return url;
    });
    const modelUrl = URL.createObjectURL(
      new Blob([JSON.stringify(model)], { type: "application/json" }),
    );
    urls.push(modelUrl);
    return { url: modelUrl, revoke() { for (const url of urls) URL.revokeObjectURL(url); } };
  } catch (error) {
    for (const url of urls) URL.revokeObjectURL(url);
    throw error;
  }
}

function stripWindowsExtendedPath(path: string) {
  return path.replace(/^\\\\\?\\/, "");
}

function resolveModelAssetPath(modelPath: string, relativePath: string) {
  const normalizedModel = stripWindowsExtendedPath(modelPath).replaceAll("\\", "/");
  const normalizedRelative = relativePath.replaceAll("\\", "/");
  if (/^[a-zA-Z]:\//.test(normalizedRelative) || normalizedRelative.startsWith("/")) {
    throw new Error(`Live2D resource must use a relative path: ${relativePath}`);
  }

  const parts = normalizedModel.split("/");
  parts.pop();
  for (const part of normalizedRelative.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (parts.length <= 1) throw new Error(`Live2D resource escapes its model directory: ${relativePath}`);
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.join("\\");
}

/** Direct Tauri asset-protocol URL — textures and moc stay on disk; only the
  * rewritten model JSON becomes a small blob URL for the loader. */
export async function createLive2DAssetModelUrl(modelPath: string): Promise<Live2DBlobModelUrl> {
  const normalizedModelPath = stripWindowsExtendedPath(modelPath);
  const response = await fetch(convertFileSrc(normalizedModelPath));
  if (!response.ok) {
    throw new Error(`Live2D model settings request failed (${response.status}).`);
  }
  const model = (await response.json()) as Record<string, unknown>;
  rewriteModelReferences(model, (path) =>
    convertFileSrc(resolveModelAssetPath(normalizedModelPath, path)),
  );
  const modelUrl = URL.createObjectURL(
    new Blob([JSON.stringify(model)], { type: "application/json" }),
  );
  return { url: modelUrl, revoke() { URL.revokeObjectURL(modelUrl); } };
}

// ----------------------------------------------------------------- subset type declarations

export interface Live2DPixiApp {
  destroy(...args: unknown[]): void;
  render?: () => void;
  renderer?: { resize(width: number, height: number): void };
  ticker?: { maxFPS: number; start(): void; stop(): void };
  stage: { addChild(child: unknown): unknown };
  screen: { width: number; height: number };
}

export interface Live2DModelLike {
  anchor: { set(x: number, y?: number): void };
  position: { set(x: number, y?: number): void };
  scale: { set(x: number, y?: number): void };
  width: number;
  height: number;
  x: number;
  y: number;
  automator?: { autoFocus: boolean };
  focus?(x: number, y: number, instant?: boolean): void;
  internalModel?: {
    width?: number;
    height?: number;
    originalWidth?: number;
    originalHeight?: number;
  };
  textures?: Array<{ source?: unknown } | null>;
}

// ----------------------------------------------------------------- engine bootstrap

async function loadEngineModules() {
  await ensureCubismCore();
  return Promise.all([import("pixi.js"), import("untitled-pixi-live2d-engine/cubism")]);
}

async function ensureLive2DEngine(onProgress?: Live2DProgressListener) {
  onProgress?.(18, "core");
  engineLoading ??= loadEngineModules();
  const modules = await engineLoading;
  const [{ extensions }, { configureCubismSDK, Live2DPlugin }] = modules;
  onProgress?.(48, "engine");

  if (!engineInitialized) {
    configureCubismSDK({ memorySizeMB: 128 });
    extensions.add(Live2DPlugin);
    engineInitialized = true;
  }
  return modules;
}

export async function preloadLive2DEngine(): Promise<void> {
  await ensureLive2DEngine();
}

export function setMouseFollowEnabled(model: Live2DModelLike, enabled: boolean) {
  if (model.automator) model.automator.autoFocus = enabled;
  if (!enabled) model.focus?.(0, 0);
}

export async function loadLive2DModel(
  modelUrl: string,
  canvas: HTMLCanvasElement,
  mouseFollow = true,
  onProgress?: Live2DProgressListener,
): Promise<Live2DLoadResult> {
  const [{ Application }, { Live2DModel }] = await ensureLive2DEngine(onProgress);

  onProgress?.(58, "renderer");
  const app = new Application();
  await app.init({
    canvas,
    resizeTo: canvas.parentElement ?? undefined,
    preference: "webgl",
    autoDensity: true,
    resolution: window.devicePixelRatio,
    backgroundAlpha: 0,
    antialias: true,
  });
  app.ticker.maxFPS = 30;

  onProgress?.(72, "model");
  const model = await Live2DModel.from(modelUrl, {
    textureOptions: { lod: false },
    autoUpdate: true,
    autoFocus: mouseFollow,
  });
  const missingTextureIndex = model.textures.findIndex((texture) => !texture?.source);
  if (missingTextureIndex >= 0) {
    model.destroy();
    app.destroy(true);
    throw new Error(`Live2D texture ${missingTextureIndex + 1} failed to load.`);
  }

  onProgress?.(100, "ready");
  return { app, model };
}

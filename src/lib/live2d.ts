// Live2D 引擎单例初始化模块。
//
// 所有 pixi.js 与 untitled-pixi-live2d-engine 的 import 都是动态的，
// 确保它们只会在用户打开 Live2D 面板时才加载，不进入主 bundle。
//
// Cubism Core（live2dcubismcore.min.js）是 Live2D 私有运行时（WASM 内嵌），
// 许可证禁止再分发，由用户通过 `npm run fetch:cubism-core` 自行下载到 public/core/，
// 运行时由本模块动态注入 <script> 标签加载。

// 模块级守卫：Live2DPlugin 只注册一次（面板重挂/重载时复用）。
let engineInitialized = false;
let coreLoading: Promise<void> | null = null;

declare global {
  interface Window {
    Live2DCubismCore?: unknown;
  }
}

/** 动态加载 Cubism Core 脚本（若尚未加载）。多次调用会复用同一个 Promise。 */
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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  app: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  model: any;
}

/**
 * 初始化 Pixi v8 + Live2D 引擎并加载模型。
 *
 * 注意：extensions.add(Live2DPlugin) 必须在 app.init() 之前注册（否则 live2d 渲染管线不会安装）；
 * preference 必须为 'webgl'（Live2D 渲染管线仅 WebGL）；
 * Application 在 Pixi v8 是异步的，需 await app.init()。
 */
export async function loadLive2DModel(
  modelUrl: string,
  canvas: HTMLCanvasElement,
): Promise<Live2DLoadResult> {
  await ensureCubismCore();

  const [{ Application, extensions }, { configureCubismSDK, Live2DModel, Live2DPlugin }] =
    await Promise.all([import("pixi.js"), import("untitled-pixi-live2d-engine/cubism")]);

  if (!engineInitialized) {
    extensions.add(Live2DPlugin);
    engineInitialized = true;
  }

  const app = new Application();
  await app.init({
    canvas,
    resizeTo: canvas.parentElement ?? undefined,
    preference: "webgl",
    autoDensity: true,
    resolution: window.devicePixelRatio,
    backgroundAlpha: 0,
  });

  // 复杂/4096 纹理模型需要更大工作内存（默认 16MB 可能不够）。
  configureCubismSDK({ memorySizeMB: 32 });

  const model = await Live2DModel.from(modelUrl, {
    textureOptions: { lod: "single-auto" },
    autoUpdate: true,
  });

  return { app, model };
}

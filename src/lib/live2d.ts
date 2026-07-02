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
  app: Live2DPixiApp;
  model: Live2DModelLike;
}

/**
 * Live2D 面板实际调用的 Pixi Application 子集（动态 import 的真实类型较重，
 * 这里只声明用到的方法/字段，以获得基本类型安全而不引入全量类型依赖）。
 * destroy / addChild 采用宽松入参，确保 pixi 的 Application / Container
 * 可结构化赋值到本接口而不触发严格函数类型冲突。
 */
export interface Live2DPixiApp {
  // 方法语法 → 参数按双变比较，pixi Application 的 destroy / Container.addChild
  // 可结构化赋值到本接口（否则 strictFunctionTypes 下严格逆变会失败）。
  destroy(...args: unknown[]): void;
  stage: { addChild(child: unknown): unknown };
  screen: { width: number; height: number };
}

/**
 * Live2D 模型实际调用的成员子集。anchor / position / scale 都是 Pixi 的
 * ObservablePoint，这里只暴露用到的 set(...)；x / y 为可读写坐标。
 */
export interface Live2DModelLike {
  anchor: { set(x: number, y?: number): void };
  position: { set(x: number, y?: number): void };
  scale: { set(x: number, y?: number): void };
  x: number;
  y: number;
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

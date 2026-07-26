import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import {
  createLive2DAssetModelUrl,
  loadLive2DModel,
  setMouseFollowEnabled,
  type Live2DBlobModelUrl,
  type Live2DLoadStage,
  type Live2DModelLike,
  type Live2DPixiApp,
} from "../lib/live2d";
import { useI18n } from "../lib/i18n";
import { CloseIcon, ExternalLinkIcon, MousePointerIcon, RotateCwIcon } from "./Icons";

type Status = "idle" | "loading" | "ready" | "error";

interface Props {
  packId: string;
  active?: boolean;
  windowMode?: boolean;
  onOpenSettings?: () => void;
  onDetached?: () => void;
  onCloseWindow?: () => void;
}

const MOUSE_FOLLOW_KEY = "demiurge-live2d-mouse-follow";

const stageKeys: Record<Live2DLoadStage | "path", string> = {
  path: "live2d.progress.path",
  core: "live2d.progress.core",
  engine: "live2d.progress.engine",
  renderer: "live2d.progress.renderer",
  model: "live2d.progress.model",
  ready: "live2d.progress.ready",
};

function initialMouseFollow() {
  try {
    return localStorage.getItem(MOUSE_FOLLOW_KEY) !== "false";
  } catch {
    return true;
  }
}

function positiveNumber(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : null;
}

function getModelSize(model: Live2DModelLike) {
  const candidates = [
    {
      source: "original",
      width: positiveNumber(model.internalModel?.originalWidth),
      height: positiveNumber(model.internalModel?.originalHeight),
    },
    {
      source: "layout",
      width: positiveNumber(model.internalModel?.width),
      height: positiveNumber(model.internalModel?.height),
    },
    {
      source: "bounds",
      width: positiveNumber(model.width),
      height: positiveNumber(model.height),
    },
  ];
  return candidates.find((item) => item.width && item.height) ?? null;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

export default function Live2DPanel({
  packId,
  active = true,
  windowMode = false,
  onOpenSettings,
  onDetached,
  onCloseWindow,
}: Props) {
  const { t } = useI18n();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const appRef = useRef<Live2DPixiApp | null>(null);
  const modelRef = useRef<Live2DModelLike | null>(null);
  const modelUrlRef = useRef<Live2DBlobModelUrl | null>(null);
  const loadIdRef = useRef(0);
  const scaleRef = useRef(1.0);
  const activeRef = useRef(active);
  const mouseFollowRef = useRef(initialMouseFollow());

  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [debugInfo, setDebugInfo] = useState("");
  const [scale, setScale] = useState(1.0);
  const [canvasRevision, setCanvasRevision] = useState(0);
  const [progress, setProgress] = useState(0);
  const [progressStage, setProgressStage] = useState<Live2DLoadStage | "path">("path");
  const [mouseFollow, setMouseFollow] = useState(initialMouseFollow);

  const fitModel = useCallback(
    (app: Live2DPixiApp, model: Live2DModelLike, zoom: number) => {
      const viewport = canvasRef.current?.parentElement;
      const screenWidth = Math.max(1, viewport?.clientWidth ?? app.screen.width);
      const screenHeight = Math.max(1, viewport?.clientHeight ?? app.screen.height);
      app.renderer?.resize(screenWidth, screenHeight);

      const modelSize = getModelSize(model);
      model.anchor.set(0.5);
      model.position.set(screenWidth / 2, screenHeight / 2);

      if (!modelSize?.width || !modelSize.height) {
        model.scale.set(zoom);
        app.render?.();
        return `screen ${Math.round(screenWidth)}x${Math.round(
          screenHeight,
        )}, model size unknown, scale ${zoom.toFixed(2)}`;
      }

      const fitScale = Math.min(
        (screenWidth * 0.78) / modelSize.width,
        (screenHeight * 0.9) / modelSize.height,
      );
      const finalScale = clamp(fitScale * zoom, 0.02, 3);
      model.scale.set(finalScale);
      app.render?.();

      return `screen ${Math.round(screenWidth)}x${Math.round(
        screenHeight,
      )}, ${modelSize.source} ${Math.round(modelSize.width)}x${Math.round(
        modelSize.height,
      )}, scale ${finalScale.toFixed(3)}`;
    },
    [],
  );

  const fitCurrentModel = useCallback(() => {
    if (!activeRef.current || !appRef.current || !modelRef.current) return;
    setDebugInfo(fitModel(appRef.current, modelRef.current, scaleRef.current));
  }, [fitModel]);

  const disposeCurrentModel = useCallback(() => {
    const app = appRef.current;
    const modelUrl = modelUrlRef.current;
    appRef.current = null;
    modelRef.current = null;
    modelUrlRef.current = null;
    app?.destroy(
      { removeView: false },
      { children: true, texture: false, textureSource: false },
    );
    modelUrl?.revoke();
  }, []);

  const loadModel = useCallback(async () => {
    if (!canvasRef.current) return;
    const loadId = ++loadIdRef.current;
    const canvas = canvasRef.current;
    setStatus("loading");
    setProgress(4);
    setProgressStage("path");
    setError("");
    setDebugInfo("");
    let pendingModelUrl: Live2DBlobModelUrl | null = null;

    try {
      const modelPath = await api.resolvePackLive2dPath(packId);
      if (loadId !== loadIdRef.current) return;
      pendingModelUrl = await createLive2DAssetModelUrl(modelPath);
      if (loadId !== loadIdRef.current) {
        pendingModelUrl.revoke();
        return;
      }
      setProgress(12);

      const { app, model } = await loadLive2DModel(
        pendingModelUrl.url,
        canvas,
        mouseFollowRef.current,
        (nextProgress, stage) => {
          if (loadId !== loadIdRef.current) return;
          setProgress(nextProgress);
          setProgressStage(stage);
        },
      );
      if (loadId !== loadIdRef.current) {
        app.destroy(
          { removeView: false },
          { children: true, texture: false, textureSource: false },
        );
        pendingModelUrl.revoke();
        return;
      }

      appRef.current = app;
      modelRef.current = model;
      modelUrlRef.current = pendingModelUrl;
      pendingModelUrl = null;
      app.stage.addChild(model);
      if (!activeRef.current) app.ticker?.stop();
      setDebugInfo(fitModel(app, model, scaleRef.current));
      requestAnimationFrame(fitCurrentModel);
      setStatus("ready");
    } catch (e) {
      pendingModelUrl?.revoke();
      if (loadId !== loadIdRef.current) return;
      setStatus("error");
      setError(String(e));
    }
  }, [fitCurrentModel, fitModel, packId]);

  useEffect(() => {
    void loadModel();
    return () => {
      loadIdRef.current += 1;
      disposeCurrentModel();
    };
  }, [canvasRevision, disposeCurrentModel, loadModel]);

  useEffect(() => {
    activeRef.current = active;
    if (active) {
      appRef.current?.ticker?.start();
      requestAnimationFrame(fitCurrentModel);
    } else {
      appRef.current?.ticker?.stop();
    }
  }, [active, fitCurrentModel]);

  useEffect(() => {
    scaleRef.current = scale;
    fitCurrentModel();
  }, [fitCurrentModel, scale]);

  useEffect(() => {
    mouseFollowRef.current = mouseFollow;
    if (modelRef.current) setMouseFollowEnabled(modelRef.current, mouseFollow);
    try {
      localStorage.setItem(MOUSE_FOLLOW_KEY, String(mouseFollow));
    } catch {
      // Optional.
    }
  }, [mouseFollow]);

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === MOUSE_FOLLOW_KEY) setMouseFollow(event.newValue !== "false");
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  useEffect(() => {
    const viewport = canvasRef.current?.parentElement;
    if (!viewport) return;
    const observer = new ResizeObserver(() => fitCurrentModel());
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [fitCurrentModel]);

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (
        windowMode &&
        event.target instanceof HTMLElement &&
        event.target.closest("[data-tauri-drag-region]")
      ) {
        return;
      }
      const model = modelRef.current;
      if (!model) return;
      const startX = event.clientX;
      const startY = event.clientY;
      const origX = model.x;
      const origY = model.y;
      const onMove = (nextEvent: PointerEvent) => {
        model.x = origX + (nextEvent.clientX - startX);
        model.y = origY + (nextEvent.clientY - startY);
      };
      const onUp = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [windowMode],
  );

  const detach = useCallback(async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    try {
      await api.openLive2dWindow();
      onDetached?.();
    } catch (reason) {
      setStatus("error");
      setError(String(reason));
    }
  }, [onDetached]);

  const noModel = status === "error" && error.includes("未配置");

  return (
    <div className={`live2d-panel flex h-full flex-col ${windowMode ? "is-window" : ""}`}>
      <div className="live2d-toolbar flex h-12 shrink-0 items-center gap-2">
        <span
          className="live2d-toolbar-title text-[14px] font-semibold"
          data-tauri-drag-region={windowMode ? "" : undefined}
        >
          {t("nav.live2d")}
        </span>
        <div className="ml-auto flex items-center gap-1.5">
          <button
            type="button"
            className="live2d-follow-toggle md-icon-button grid size-8 place-items-center"
            aria-pressed={mouseFollow}
            aria-label={t("live2d.mouseFollow")}
            title={t("live2d.mouseFollow")}
            onClick={() => setMouseFollow((enabled) => !enabled)}
          >
            <MousePointerIcon size={15} />
          </button>
          <label className="live2d-scale-control flex items-center gap-1.5 text-[12px]">
            <span>{t("live2d.scale")}</span>
            <input
              type="range"
              min={0.2}
              max={3}
              step={0.05}
              value={scale}
              onChange={(event) => setScale(Number(event.target.value))}
              className="w-24"
            />
          </label>
          {!windowMode && (
            <button
              type="button"
              onClick={() => void detach()}
              className="md-icon-button grid size-8 place-items-center"
              aria-label={t("live2d.detach")}
              title={t("live2d.detach")}
            >
              <ExternalLinkIcon size={17} />
            </button>
          )}
          <button
            type="button"
            onClick={() => setCanvasRevision((revision) => revision + 1)}
            disabled={status === "loading"}
            className="md-icon-button grid size-8 place-items-center disabled:cursor-wait disabled:opacity-45"
            aria-label={t("live2d.reload")}
            title={t("live2d.reload")}
          >
            <RotateCwIcon size={17} />
          </button>
          {windowMode && onCloseWindow && (
            <button
              type="button"
              onClick={onCloseWindow}
              className="md-icon-button grid size-8 place-items-center"
              aria-label={t("live2d.closeWindow")}
              title={t("live2d.closeWindow")}
            >
              <CloseIcon size={17} />
            </button>
          )}
        </div>
      </div>

      <div className="live2d-stage relative min-h-0 flex-1" onPointerDown={onPointerDown}>
        {status === "loading" && (
          <div className="live2d-loading absolute inset-0 z-10 grid place-items-center">
            <div className="live2d-loading-surface">
              <div className="flex items-baseline justify-between gap-6">
                <span className="text-[13px] font-medium">{t(stageKeys[progressStage])}</span>
                <span className="text-[12px] tabular-nums">{progress}%</span>
              </div>
              <div
                className="live2d-progress"
                role="progressbar"
                aria-label={t("live2d.loading")}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={progress}
              >
                <span style={{ width: `${progress}%` }} />
              </div>
            </div>
          </div>
        )}
        {status === "error" && (
          <div className="absolute inset-0 z-10 grid place-items-center p-6 text-center">
            <div className="live2d-error-surface flex flex-col items-center gap-3">
              <div
                className={`max-w-[80%] text-[13px] leading-6 ${noModel ? "" : "text-[#b42318]"}`}
              >
                {noModel ? t("live2d.noModel") : t("live2d.loadFailed", { error })}
              </div>
              {onOpenSettings && (
                <button type="button" onClick={onOpenSettings} className="md-button md-button-outlined">
                  {t("live2d.goConfig")}
                </button>
              )}
            </div>
          </div>
        )}
        {import.meta.env.DEV && status === "ready" && debugInfo && !windowMode && (
          <div className="live2d-debug pointer-events-none absolute bottom-3 left-3 max-w-[70%] px-2.5 py-1.5 text-[11px]">
            Live2D ready: {debugInfo}
          </div>
        )}
        <canvas key={canvasRevision} ref={canvasRef} className="h-full w-full touch-none" />
      </div>
    </div>
  );
}

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import {
  createLive2DBlobModelUrl,
  loadLive2DModel,
  type Live2DBlobModelUrl,
  type Live2DModelLike,
  type Live2DPixiApp,
} from "../lib/live2d";
import { useI18n } from "../lib/i18n";
import { RotateCwIcon } from "./Icons";

type Status = "idle" | "loading" | "ready" | "error";

interface Props {
  packId: string;
  onOpenSettings?: () => void;
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

export default function Live2DPanel({ packId, onOpenSettings }: Props) {
  const { t } = useI18n();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const appRef = useRef<Live2DPixiApp | null>(null);
  const modelRef = useRef<Live2DModelLike | null>(null);
  const blobModelRef = useRef<Live2DBlobModelUrl | null>(null);
  const loadIdRef = useRef(0);
  const scaleRef = useRef(1.0);

  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [debugInfo, setDebugInfo] = useState("");
  const [scale, setScale] = useState(1.0);
  const [canvasRevision, setCanvasRevision] = useState(0);

  const fitModel = useCallback((app: Live2DPixiApp, model: Live2DModelLike, zoom: number) => {
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
      return `screen ${Math.round(screenWidth)}x${Math.round(screenHeight)}, model size unknown, scale ${zoom.toFixed(2)}`;
    }

    const fitScale = Math.min((screenWidth * 0.78) / modelSize.width, (screenHeight * 0.9) / modelSize.height);
    const finalScale = clamp(fitScale * zoom, 0.02, 3);
    model.scale.set(finalScale);
    app.render?.();

    return `screen ${Math.round(screenWidth)}x${Math.round(screenHeight)}, ${modelSize.source} ${Math.round(
      modelSize.width,
    )}x${Math.round(modelSize.height)}, scale ${finalScale.toFixed(3)}`;
  }, []);

  const fitCurrentModel = useCallback(() => {
    if (!appRef.current || !modelRef.current) return;
    setDebugInfo(fitModel(appRef.current, modelRef.current, scaleRef.current));
  }, [fitModel]);

  const disposeCurrentModel = useCallback(() => {
    const app = appRef.current;
    const blobModel = blobModelRef.current;
    appRef.current = null;
    modelRef.current = null;
    blobModelRef.current = null;

    // The canvas belongs to React. Removing it here leaves canvasRef pointing at
    // a detached element, so a subsequent reload renders off-screen.
    app?.destroy(
      { removeView: false },
      { children: true, texture: false, textureSource: false },
    );
    blobModel?.revoke();
  }, []);

  const loadModel = useCallback(async () => {
    if (!canvasRef.current) return;
    const loadId = ++loadIdRef.current;
    const canvas = canvasRef.current;
    setStatus("loading");
    setError("");
    setDebugInfo("");
    let pendingBlobModel: Live2DBlobModelUrl | null = null;
    try {
      const bundle = await api.packLive2dBundle(packId);
      if (loadId !== loadIdRef.current) return;

      pendingBlobModel = createLive2DBlobModelUrl(bundle);
      const { app, model } = await loadLive2DModel(pendingBlobModel.url, canvas);
      if (loadId !== loadIdRef.current) {
        app.destroy(
          { removeView: false },
          { children: true, texture: false, textureSource: false },
        );
        pendingBlobModel.revoke();
        return;
      }

      blobModelRef.current = pendingBlobModel;
      pendingBlobModel = null;
      appRef.current = app;
      modelRef.current = model;
      app.stage.addChild(model);
      setDebugInfo(fitModel(app, model, scaleRef.current));
      requestAnimationFrame(fitCurrentModel);
      setStatus("ready");
    } catch (e) {
      pendingBlobModel?.revoke();
      if (loadId !== loadIdRef.current) return;
      setStatus("error");
      setError(String(e));
    }
  }, [disposeCurrentModel, fitCurrentModel, fitModel, packId]);

  useEffect(() => {
    void loadModel();
    return () => {
      loadIdRef.current += 1;
      disposeCurrentModel();
    };
  }, [canvasRevision, disposeCurrentModel, loadModel]);

  // 缩放变化时应用到当前模型。
  useEffect(() => {
    scaleRef.current = scale;
    fitCurrentModel();
  }, [fitCurrentModel, scale]);

  useEffect(() => {
    const viewport = canvasRef.current?.parentElement;
    if (!viewport) return;
    const observer = new ResizeObserver(() => fitCurrentModel());
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [fitCurrentModel]);

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const model = modelRef.current;
    if (!model) return;
    const startX = e.clientX;
    const startY = e.clientY;
    const origX = model.x;
    const origY = model.y;
    const onMove = (ev: PointerEvent) => {
      model.x = origX + (ev.clientX - startX);
      model.y = origY + (ev.clientY - startY);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }, []);

  const noModel = status === "error" && error.includes("未配置");

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-[#eceff3] bg-[#fbfcfd] px-3">
        <span className="text-[14px] font-semibold text-[#202124]">{t("nav.live2d")}</span>
        <div className="ml-auto flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-[12px] text-[#4f5661]">
            <span>{t("live2d.scale")}</span>
            <input
              type="range"
              min={0.2}
              max={3}
              step={0.05}
              value={scale}
              onChange={(e) => setScale(Number(e.target.value))}
              className="w-24"
            />
          </label>
          <button
            onClick={() => setCanvasRevision((revision) => revision + 1)}
            disabled={status === "loading"}
            className="grid size-8 place-items-center rounded-md text-[#4f5661] transition hover:bg-[#eef1f5] disabled:cursor-wait disabled:opacity-45"
            aria-label={t("live2d.reload")}
            title={t("live2d.reload")}
          >
            <RotateCwIcon size={17} />
          </button>
        </div>
      </div>

      <div className="relative min-h-0 flex-1" onPointerDown={onPointerDown}>
        {status === "loading" && (
          <div className="absolute inset-0 grid place-items-center text-[13px] text-[#8a9099]">
            {t("live2d.loading")}
          </div>
        )}
        {status === "error" && (
          <div className="absolute inset-0 grid place-items-center p-6 text-center">
            <div className="flex flex-col items-center gap-3">
              <div
                className={`max-w-[80%] text-[13px] leading-6 ${noModel ? "text-[#7a8088]" : "text-[#b42318]"}`}
              >
                {noModel ? t("live2d.noModel") : t("live2d.loadFailed", { error })}
              </div>
              {onOpenSettings && (
                <button
                  type="button"
                  onClick={onOpenSettings}
                  className="rounded-md border border-[#dfe3e8] bg-white px-3 py-1.5 text-[12px] font-medium text-[#3f3f3f] transition hover:bg-[#f6f7f9]"
                >
                  {t("live2d.goConfig")}
                </button>
              )}
            </div>
          </div>
        )}
        {import.meta.env.DEV && status === "ready" && debugInfo && (
          <div className="pointer-events-none absolute bottom-3 left-3 max-w-[70%] rounded-md border border-[#dfe3e8] bg-white/90 px-2.5 py-1.5 text-[11px] text-[#69707a] shadow-sm">
            Live2D ready: {debugInfo}
          </div>
        )}
        <canvas key={canvasRevision} ref={canvasRef} className="h-full w-full touch-none" />
      </div>
    </div>
  );
}

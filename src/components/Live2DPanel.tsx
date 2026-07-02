import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as api from "../lib/api";
import { loadLive2DModel } from "../lib/live2d";
import { useI18n } from "../lib/i18n";
import { RotateCwIcon } from "./Icons";

type Status = "idle" | "loading" | "ready" | "error";

interface Props {
  packId: string;
  onOpenSettings?: () => void;
}

export default function Live2DPanel({ packId, onOpenSettings }: Props) {
  const { t } = useI18n();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const appRef = useRef<any>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const modelRef = useRef<any>(null);

  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [scale, setScale] = useState(1.0);

  const loadModel = useCallback(async () => {
    if (!canvasRef.current) return;
    if (appRef.current) {
      appRef.current.destroy(true);
      appRef.current = null;
      modelRef.current = null;
    }
    setStatus("loading");
    setError("");
    try {
      const absPath = await api.resolvePackLive2dPath(packId);
      const url = convertFileSrc(absPath);
      const { app, model } = await loadLive2DModel(url, canvasRef.current);
      appRef.current = app;
      modelRef.current = model;
      model.anchor.set(0.5);
      model.position.set(app.screen.width / 2, app.screen.height / 2);
      model.scale.set(scale);
      app.stage.addChild(model);
      setStatus("ready");
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  }, [packId, scale]);

  useEffect(() => {
    void loadModel();
    return () => {
      if (appRef.current) {
        appRef.current.destroy(true);
        appRef.current = null;
        modelRef.current = null;
      }
    };
  }, [loadModel]);

  // 缩放变化时应用到当前模型。
  useEffect(() => {
    if (modelRef.current) modelRef.current.scale.set(scale);
  }, [scale]);

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
            onClick={() => void loadModel()}
            className="grid size-8 place-items-center rounded-md text-[#4f5661] transition hover:bg-[#eef1f5]"
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
        <canvas ref={canvasRef} className="h-full w-full touch-none" />
      </div>
    </div>
  );
}

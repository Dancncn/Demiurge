import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import * as api from "@/lib/api";
import type { Settings } from "@/lib/types";
import { PetCanvas } from "@/features/pet/PetCanvas";
import {
  PetActivityState,
  signalFromAgentEvent,
  type PetSemanticAction,
} from "@/features/pet/petRuntime";
import { CloseIcon, MousePointerIcon, PinIcon, SettingsIcon } from "@/shared/components/Icons";
import { useI18n } from "@/lib/i18n";

function ShellButton({
  active,
  title,
  disabled,
  onClick,
  children,
}: {
  active?: boolean;
  title: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      title={title}
      aria-label={title}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      className={`desktop-companion-control grid size-7 place-items-center border text-[12px] transition ${
        active
          ? "border-[#b8d4ff] bg-white/95 text-[#0b57d0]"
          : "border-white/70 bg-white/88 text-[#59616d] hover:bg-white hover:text-[#202124]"
      } disabled:cursor-not-allowed disabled:opacity-45`}
    >
      {children}
    </button>
  );
}

export default function DesktopCompanionShell() {
  const { t } = useI18n();
  const runtimeRef = useRef(new PetActivityState());
  const dragOriginRef = useRef<{ pointerId: number; x: number; y: number; started: boolean } | null>(null);
  const lastWindowXRef = useRef<number | null>(null);
  const motionEndTimerRef = useRef<number | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [action, setAction] = useState<PetSemanticAction>("idle");
  const [revision, setRevision] = useState(0);
  const [status, setStatus] = useState("");

  useEffect(() => {
    document.documentElement.dataset.window = "desktop-companion";
    return () => {
      delete document.documentElement.dataset.window;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlistenSettings: (() => void) | undefined;
    let unlistenAgent: (() => void) | undefined;
    let unlistenCatalog: (() => void) | undefined;

    void api.getSettings().then(
      (next) => {
        if (!cancelled) setSettings(next);
      },
      (error) => {
        if (!cancelled) setStatus(String(error));
      },
    );
    void api.listenSettingsUpdated(setSettings).then((dispose) => {
      if (cancelled) dispose();
      else unlistenSettings = dispose;
    });
    void api.listenUnifiedAgentEvents((event) => {
      const signal = signalFromAgentEvent(event);
      if (signal) setAction(runtimeRef.current.dispatch(signal).action);
    }).then((dispose) => {
      if (cancelled) dispose();
      else unlistenAgent = dispose;
    });
    void api.listenPetCatalogUpdated(() => setRevision((value) => value + 1)).then((dispose) => {
      if (cancelled) dispose();
      else unlistenCatalog = dispose;
    });

    return () => {
      cancelled = true;
      unlistenSettings?.();
      unlistenAgent?.();
      unlistenCatalog?.();
    };
  }, []);

  async function updateSettings(patch: Partial<Settings>) {
    const base = settings ?? (await api.getSettings());
    const next = { ...base, ...patch };
    setSettings(next);
    setStatus("");
    try {
      await api.saveSettings(next);
    } catch (error) {
      setSettings(base);
      setStatus(String(error));
    }
  }

  const handleAnimationComplete = useCallback(() => {
    setAction(runtimeRef.current.dispatch({ type: "animation.completed" }).action);
  }, []);

  const handleTap = useCallback(() => {
    setAction(runtimeRef.current.dispatch({ type: "interaction.tap" }).action);
  }, []);

  const handleHoverChange = useCallback((hovered: boolean) => {
    const signal = hovered ? { type: "interaction.hover_started" as const } : { type: "interaction.hover_ended" as const };
    setAction(runtimeRef.current.dispatch(signal).action);
  }, []);

  const handlePetError = useCallback((message: string) => setStatus(message), []);

  const startMotion = useCallback((direction: "left" | "right") => {
    if (motionEndTimerRef.current !== null) window.clearTimeout(motionEndTimerRef.current);
    motionEndTimerRef.current = null;
    setAction(runtimeRef.current.dispatch({ type: "motion.started", direction }).action);
  }, []);

  const stopMotion = useCallback(() => {
    if (motionEndTimerRef.current !== null) window.clearTimeout(motionEndTimerRef.current);
    motionEndTimerRef.current = null;
    setAction(runtimeRef.current.dispatch({ type: "motion.ended" }).action);
  }, []);

  const scheduleMotionEnd = useCallback(() => {
    if (motionEndTimerRef.current !== null) window.clearTimeout(motionEndTimerRef.current);
    motionEndTimerRef.current = window.setTimeout(stopMotion, 180);
  }, [stopMotion]);

  useEffect(() => {
    let cancelled = false;
    let unlistenMoved: (() => void) | undefined;

    void getCurrentWindow().onMoved(({ payload }) => {
      const previousX = lastWindowXRef.current;
      if (previousX !== null && payload.x !== previousX) {
        startMotion(payload.x < previousX ? "left" : "right");
      }
      lastWindowXRef.current = payload.x;
      scheduleMotionEnd();
    }).then((dispose) => {
      if (cancelled) dispose();
      else unlistenMoved = dispose;
    });

    return () => {
      cancelled = true;
      unlistenMoved?.();
      if (motionEndTimerRef.current !== null) window.clearTimeout(motionEndTimerRef.current);
      motionEndTimerRef.current = null;
    };
  }, [scheduleMotionEnd, startMotion]);

  function prepareDragging(event: React.PointerEvent<HTMLElement>) {
    if (event.button !== 0 || (event.target as HTMLElement).closest(".desktop-companion-control")) return;
    dragOriginRef.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, started: false };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  async function dragWindow(direction: "left" | "right") {
    startMotion(direction);
    scheduleMotionEnd();
    try {
      await getCurrentWindow().startDragging();
    } catch (error) {
      setStatus(String(error));
      stopMotion();
    }
  }

  function continueDragging(event: React.PointerEvent<HTMLElement>) {
    const origin = dragOriginRef.current;
    if (!origin || origin.pointerId !== event.pointerId || origin.started) return;
    if (Math.hypot(event.clientX - origin.x, event.clientY - origin.y) < 6) return;
    origin.started = true;
    void dragWindow(event.clientX < origin.x ? "left" : "right");
  }

  function finishDragging(event: React.PointerEvent<HTMLElement>) {
    const origin = dragOriginRef.current;
    if (origin?.pointerId !== event.pointerId) return;
    dragOriginRef.current = null;
    if (origin.started) stopMotion();
  }

  function cancelDragging(event: React.PointerEvent<HTMLElement>) {
    if (dragOriginRef.current?.pointerId !== event.pointerId) return;
    dragOriginRef.current = null;
    scheduleMotionEnd();
  }

  if (!settings) {
    return <main className="h-screen w-screen bg-transparent" />;
  }

  return (
    <main
      className="desktop-companion-shell group relative h-screen w-screen overflow-hidden bg-transparent"
      onPointerDown={prepareDragging}
      onPointerMove={continueDragging}
      onPointerUp={finishDragging}
      onPointerCancel={cancelDragging}
      onDoubleClick={() => void api.desktopCompanionShowMain()}
    >
      <div className="desktop-companion-drag absolute inset-0 grid place-items-center">
        {settings.current_pet ? (
          <PetCanvas
            petId={settings.current_pet}
            action={action}
            revision={revision}
            onAnimationComplete={handleAnimationComplete}
            onTap={handleTap}
            onHoverChange={handleHoverChange}
            onError={handlePetError}
          />
        ) : (
          <button
            type="button"
            className="desktop-companion-control grid size-24 place-items-center border border-white/70 bg-white/88 shadow-lg"
            onClick={() => void api.desktopCompanionShowMain()}
            title={t("settings.pet.empty")}
          >
            <img src="/demiurge.png" alt="" className="size-20 object-contain opacity-75" />
          </button>
        )}
      </div>

      {status && (
        <div className="desktop-companion-control absolute inset-x-2 bottom-10 truncate bg-white/90 px-2 py-1 text-[11px] text-[#b42318] shadow">
          {status}
        </div>
      )}

      <footer className="desktop-companion-control absolute right-2 top-2 flex gap-1 opacity-0 transition group-hover:opacity-100">
        <ShellButton
          active={settings.desktop_companion_always_on_top}
          title={t("desktopCompanion.pin")}
          onClick={() => void updateSettings({ desktop_companion_always_on_top: !settings.desktop_companion_always_on_top })}
        >
          <PinIcon size={14} />
        </ShellButton>
        <ShellButton
          active={settings.desktop_companion_click_through}
          title={t("desktopCompanion.clickThrough")}
          onClick={() => void updateSettings({ desktop_companion_click_through: !settings.desktop_companion_click_through })}
        >
          <MousePointerIcon size={14} />
        </ShellButton>
        <ShellButton title={t("desktopCompanion.openMain")} onClick={() => void api.desktopCompanionShowMain()}>
          <SettingsIcon size={14} />
        </ShellButton>
        <ShellButton
          title={t("desktopCompanion.close")}
          onClick={() =>
            void updateSettings({
              desktop_companion_enabled: false,
              desktop_companion_click_through: false,
            })
          }
        >
          <CloseIcon size={14} />
        </ShellButton>
      </footer>
    </main>
  );
}

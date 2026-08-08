import { getCurrentWindow } from "@tauri-apps/api/window";
import { lazy, Suspense, useEffect, useState } from "react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { Settings } from "@/lib/types";

const Live2DPanel = lazy(() => import("@/features/live2d/Live2DPanel"));

const IS_TAURI = "__TAURI_INTERNALS__" in window;

const PREVIEW_SETTINGS = {
  current_pack: "default",
  current_pet: "",
  language: "zh",
  theme: "light",
  appearance: "material_bloom",
} as Settings;

export default function Live2DWindowShell() {
  const { setLang, t } = useI18n();
  const [settings, setSettings] = useState<Settings | null>(IS_TAURI ? null : PREVIEW_SETTINGS);
  const [armed, setArmed] = useState(!IS_TAURI);
  const [renderingActive, setRenderingActive] = useState(!IS_TAURI);
  const [error, setError] = useState("");

  useEffect(() => {
    document.documentElement.dataset.window = "live2d";
    return () => {
      delete document.documentElement.dataset.window;
    };
  }, []);

  useEffect(() => {
    const theme = settings?.theme ?? "system";
    const media = window.matchMedia?.("(prefers-color-scheme: dark)") ?? null;

    function applyTheme() {
      const resolved = theme === "system" ? (media?.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.theme = theme;
      document.documentElement.dataset.resolvedTheme = resolved;
      document.documentElement.style.colorScheme = resolved;
    }

    applyTheme();
    if (theme !== "system" || !media) return;
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [settings?.theme]);

  useEffect(() => {
    document.documentElement.dataset.appearance = settings?.appearance ?? "material_bloom";
  }, [settings?.appearance]);

  useEffect(() => {
    if (!IS_TAURI) {
      setLang(PREVIEW_SETTINGS.language);
      return;
    }

    const current = getCurrentWindow();
    let cancelled = false;
    let unlistenFocus: (() => void) | undefined;
    let unlistenVisibility: (() => void) | undefined;
    let unlistenSettings: (() => void) | undefined;

    const refresh = async (activate: boolean) => {
      try {
        const next = await api.getSettings();
        if (cancelled) return;
        setSettings(next);
        setLang(next.language);
        setError("");
        if (activate) setArmed(true);
      } catch (reason) {
        if (!cancelled) setError(String(reason));
      }
    };

    void current.isVisible().then((visible) => {
      if (cancelled) return;
      setRenderingActive(visible);
      if (visible) void refresh(true);
    });
    void current.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        setRenderingActive(true);
        void refresh(true);
      }
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenFocus = unlisten;
    });
    void current.listen<boolean>("live2d-visibility-changed", ({ payload: visible }) => {
      setRenderingActive(visible);
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenVisibility = unlisten;
    });
    void api.listenSettingsUpdated((next) => {
      setSettings(next);
      setLang(next.language);
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenSettings = unlisten;
    });

    return () => {
      cancelled = true;
      unlistenFocus?.();
      unlistenVisibility?.();
      unlistenSettings?.();
    };
  }, [setLang]);

  const closeWindow = () => {
    if (IS_TAURI) {
      setRenderingActive(false);
      void getCurrentWindow().close();
    }
    else window.close();
  };

  const showMainWindow = () => {
    if (IS_TAURI) void api.desktopCompanionShowMain();
  };

  return (
    <main className="live2d-window-shell">
      {armed && settings ? (
        <Suspense fallback={<div className="live2d-window-standby">{t("live2d.loading")}</div>}>
          <Live2DPanel
            packId={settings.current_pack}
            active={renderingActive}
            windowMode
            onOpenSettings={showMainWindow}
            onCloseWindow={closeWindow}
          />
        </Suspense>
      ) : (
        <div className="live2d-window-standby" data-tauri-drag-region>
          {error || t("live2d.loading")}
        </div>
      )}
    </main>
  );
}

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import { haltAudioPlayback } from "./audioPlayback";
import type { Settings, VoiceStatus } from "./types";

export interface StreamingTtsStatus {
  active: boolean;
  muted: boolean;
  speaking: boolean;
  queued: number;
  currentText: string;
  error: string;
  playedSegments: number;
}

const SENTENCE_END_RE = /[。！？!?；;]+["'”’）】》]*|\n{2,}/;
const LONG_FRAGMENT_CHARS = 180;

function ttsAvailable(settings: Settings | null, readiness: VoiceStatus | null) {
  return !!settings?.voice_enabled && readiness?.enabled === true && readiness.tts_ready;
}

function takeSentence(buffer: string, flush: boolean): { sentence: string; rest: string } | null {
  const match = SENTENCE_END_RE.exec(buffer);
  if (match?.index !== undefined) {
    const end = match.index + match[0].length;
    return { sentence: buffer.slice(0, end).trim(), rest: buffer.slice(end) };
  }
  if (!flush && buffer.length > LONG_FRAGMENT_CHARS) {
    const pivot = Math.max(buffer.lastIndexOf("，"), buffer.lastIndexOf(","), buffer.lastIndexOf(" "));
    if (pivot > 24) return { sentence: buffer.slice(0, pivot + 1).trim(), rest: buffer.slice(pivot + 1) };
  }
  if (flush && buffer.trim()) return { sentence: buffer.trim(), rest: "" };
  return null;
}

export function useStreamingTtsQueue(settings: Settings | null) {
  const settingsRef = useRef(settings);
  const queueRef = useRef<string[]>([]);
  const bufferRef = useRef("");
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const finishAudioRef = useRef<(() => void) | null>(null);
  const drainingRef = useRef(false);
  const drainRef = useRef<(() => Promise<void>) | null>(null);
  const generationRef = useRef(0);
  const activeRef = useRef(false);
  const mutedRef = useRef(false);
  const readinessRef = useRef<VoiceStatus | null>(null);
  const [available, setAvailable] = useState(false);

  const [status, setStatus] = useState<StreamingTtsStatus>({
    active: false,
    muted: false,
    speaking: false,
    queued: 0,
    currentText: "",
    error: "",
    playedSegments: 0,
  });

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  const refreshStatus = useCallback((patch: Partial<StreamingTtsStatus> = {}) => {
    setStatus((current) => ({
      ...current,
      active: activeRef.current,
      muted: mutedRef.current,
      queued: queueRef.current.length,
      ...patch,
    }));
  }, []);

  useEffect(() => {
    let cancelled = false;
    readinessRef.current = null;
    setAvailable(false);
    if (!settings?.voice_enabled) return;
    void api
      .voiceStatus()
      .then((next) => {
        if (cancelled) return;
        readinessRef.current = next;
        setAvailable(ttsAvailable(settingsRef.current, next));
      })
      .catch((error) => {
        if (!cancelled) refreshStatus({ error: String(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [settings, refreshStatus]);

  const stopAudio = useCallback(() => {
    generationRef.current += 1;
    const finishAudio = finishAudioRef.current;
    finishAudioRef.current = null;
    const audio = audioRef.current;
    audioRef.current = null;
    if (audio) {
      audio.onended = null;
      audio.onerror = null;
      haltAudioPlayback(audio);
    }
    finishAudio?.();
  }, []);

  const drain = useCallback(async () => {
    if (drainingRef.current) return;
    drainingRef.current = true;
    try {
      while (activeRef.current && !mutedRef.current && queueRef.current.length > 0) {
        const text = queueRef.current.shift() ?? "";
        if (!text.trim()) continue;
        const settings = settingsRef.current;
        if (!ttsAvailable(settings, readinessRef.current)) {
          refreshStatus({ error: "TTS backend is not ready.", speaking: false, currentText: "" });
          break;
        }
        const ticket = generationRef.current;
        refreshStatus({ speaking: true, currentText: text, error: "" });
        try {
          const url = await api.voiceSynthesize(text, settings?.voice_id || undefined, {
            speed: settings?.voice_speed,
            emotion: settings?.voice_emotion,
            streaming: settings?.voice_streaming,
          });
          if (ticket !== generationRef.current || mutedRef.current || !activeRef.current) break;
          await new Promise<void>((resolve, reject) => {
            let settled = false;
            const audio = new Audio(url);
            let cancelPlayback: () => void = () => undefined;
            const settle = (fn: () => void) => {
              if (settled) return;
              settled = true;
              if (finishAudioRef.current === cancelPlayback) finishAudioRef.current = null;
              if (audioRef.current === audio) audioRef.current = null;
              audio.onended = null;
              audio.onerror = null;
              fn();
            };
            cancelPlayback = () => settle(resolve);
            const failPlayback = (err: unknown) =>
              settle(() => reject(err instanceof Error ? err : new Error(String(err))));
            audioRef.current = audio;
            finishAudioRef.current = cancelPlayback;
            audio.onended = () => settle(resolve);
            audio.onerror = () => failPlayback(new Error("Audio playback failed."));
            void audio.play().catch(failPlayback);
          });
          if (ticket === generationRef.current) {
            setStatus((current) => ({
              ...current,
              active: activeRef.current,
              muted: mutedRef.current,
              queued: queueRef.current.length,
              playedSegments: current.playedSegments + 1,
            }));
          }
        } catch (err) {
          refreshStatus({ error: String(err), speaking: false, currentText: "" });
          break;
        } finally {
          if (ticket === generationRef.current) audioRef.current = null;
        }
      }
    } finally {
      drainingRef.current = false;
      refreshStatus({ speaking: false, currentText: "" });
      if (activeRef.current && !mutedRef.current && queueRef.current.length > 0) {
        window.setTimeout(() => void drainRef.current?.(), 0);
      }
    }
  }, [refreshStatus]);

  useEffect(() => {
    drainRef.current = drain;
  }, [drain]);

  const enqueue = useCallback(
    (text: string) => {
      const sentence = text.trim();
      if (!sentence || !activeRef.current || mutedRef.current) return;
      queueRef.current.push(sentence);
      refreshStatus();
      void drain();
    },
    [drain, refreshStatus],
  );

  const beginTurn = useCallback(
    (active: boolean) => {
      stopAudio();
      queueRef.current = [];
      bufferRef.current = "";
      activeRef.current = active && ttsAvailable(settingsRef.current, readinessRef.current);
      refreshStatus({ speaking: false, currentText: "", error: "" });
    },
    [refreshStatus, stopAudio],
  );

  const pushText = useCallback(
    (delta: string) => {
      if (!activeRef.current || mutedRef.current || !delta) return;
      bufferRef.current += delta;
      let next = takeSentence(bufferRef.current, false);
      while (next) {
        bufferRef.current = next.rest;
        enqueue(next.sentence);
        next = takeSentence(bufferRef.current, false);
      }
    },
    [enqueue],
  );

  const flush = useCallback(() => {
    if (!activeRef.current || mutedRef.current) return;
    const next = takeSentence(bufferRef.current, true);
    bufferRef.current = next?.rest ?? "";
    if (next?.sentence) enqueue(next.sentence);
  }, [enqueue]);

  const stop = useCallback(() => {
    stopAudio();
    queueRef.current = [];
    bufferRef.current = "";
    activeRef.current = false;
    refreshStatus({ speaking: false, currentText: "", error: "" });
  }, [refreshStatus, stopAudio]);

  const setMuted = useCallback(
    (muted: boolean) => {
      mutedRef.current = muted;
      if (muted) {
        stopAudio();
        queueRef.current = [];
        bufferRef.current = "";
      }
      refreshStatus({ speaking: false, currentText: muted ? "" : status.currentText });
    },
    [refreshStatus, status.currentText, stopAudio],
  );

  const speakText = useCallback(
    (text: string) => {
      beginTurn(true);
      pushText(text);
      flush();
    },
    [beginTurn, flush, pushText],
  );

  useEffect(() => stop, [stop]);

  return {
    available,
    status,
    beginTurn,
    pushText,
    flush,
    stop,
    setMuted,
    speakText,
  };
}

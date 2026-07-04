import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import type { Settings } from "./types";

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

function ttsAvailable(settings: Settings | null) {
  return !!settings?.voice_enabled && !!settings.voice_tts_backend && settings.voice_tts_backend !== "none";
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
  const drainingRef = useRef(false);
  const generationRef = useRef(0);
  const activeRef = useRef(false);
  const mutedRef = useRef(false);

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

  const stopAudio = useCallback(() => {
    generationRef.current += 1;
    const audio = audioRef.current;
    audioRef.current = null;
    if (audio) {
      audio.onended = null;
      audio.onerror = null;
      audio.pause();
      audio.src = "";
    }
  }, []);

  const drain = useCallback(async () => {
    if (drainingRef.current) return;
    drainingRef.current = true;
    try {
      while (activeRef.current && !mutedRef.current && queueRef.current.length > 0) {
        const text = queueRef.current.shift() ?? "";
        if (!text.trim()) continue;
        const settings = settingsRef.current;
        if (!ttsAvailable(settings)) {
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
            const audio = new Audio(url);
            audioRef.current = audio;
            audio.playbackRate = settings?.voice_speed || 1;
            audio.onended = () => resolve();
            audio.onerror = () => reject(new Error("Audio playback failed."));
            void audio.play().catch(reject);
          });
          if (ticket === generationRef.current) {
            refreshStatus({ playedSegments: status.playedSegments + 1 });
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
      refreshStatus({ speaking: false, currentText: queueRef.current.length ? status.currentText : "" });
    }
  }, [refreshStatus, status.currentText, status.playedSegments]);

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
      activeRef.current = active && ttsAvailable(settingsRef.current);
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
    available: ttsAvailable(settings),
    status,
    beginTurn,
    pushText,
    flush,
    stop,
    setMuted,
    speakText,
  };
}

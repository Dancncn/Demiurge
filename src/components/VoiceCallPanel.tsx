import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import type { Settings } from "../lib/types";
import type { StreamingTtsStatus } from "../lib/useStreamingTtsQueue";
import { CloseIcon, MicIcon, PhoneIcon, StopIcon, VolumeIcon } from "./Icons";
import { useI18n } from "../lib/i18n";

type Props = {
  open: boolean;
  active: boolean;
  muted: boolean;
  busy: boolean;
  startedAt: number | null;
  settings: Settings | null;
  ttsStatus: StreamingTtsStatus;
  onStart: () => void;
  onEnd: () => void;
  onClose: () => void;
  onMutedChange: (muted: boolean) => void;
  onTranscript: (text: string) => Promise<boolean> | boolean;
  onStopAudio: () => void;
};

function recordingSupported() {
  return (
    typeof navigator !== "undefined" &&
    !!navigator.mediaDevices?.getUserMedia &&
    typeof window !== "undefined" &&
    typeof window.MediaRecorder !== "undefined"
  );
}

function formatDuration(startedAt: number | null, now: number) {
  if (!startedAt) return "00:00";
  const total = Math.max(0, Math.floor((now - startedAt) / 1000));
  const mm = String(Math.floor(total / 60)).padStart(2, "0");
  const ss = String(total % 60).padStart(2, "0");
  return `${mm}:${ss}`;
}

export default function VoiceCallPanel({
  open,
  active,
  muted,
  busy,
  startedAt,
  settings,
  ttsStatus,
  onStart,
  onEnd,
  onClose,
  onMutedChange,
  onTranscript,
  onStopAudio,
}: Props) {
  const { t } = useI18n();
  const supported = recordingSupported();
  const [recording, setRecording] = useState(false);
  const [transcribing, setTranscribing] = useState(false);
  const [error, setError] = useState("");
  const [lastTranscript, setLastTranscript] = useState("");
  const [now, setNow] = useState(Date.now());
  const [vadEnabled, setVadEnabled] = useState(true);

  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const vadTimerRef = useRef<number | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const recordingTokenRef = useRef(0);
  const discardRecordingRef = useRef(false);
  const transcriptionTokenRef = useRef(0);

  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active]);

  useEffect(() => {
    if (!open || !active) return;
    const onHotkey = () => {
      void toggleRecording();
    };
    window.addEventListener("demiurge-voice-hotkey", onHotkey);
    return () => window.removeEventListener("demiurge-voice-hotkey", onHotkey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, active, recording, transcribing, busy]);

  useEffect(() => {
    return () => {
      discardRecordingRef.current = true;
      transcriptionTokenRef.current += 1;
      chunksRef.current = [];
      stopVad();
      releaseStream();
      try {
        recorderRef.current?.stop();
      } catch {
        /* ignore */
      }
      recorderRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!open) cancelVoiceInput();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function releaseStream() {
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
  }

  function stopVad() {
    if (vadTimerRef.current) {
      window.clearInterval(vadTimerRef.current);
      vadTimerRef.current = null;
    }
    void audioContextRef.current?.close().catch(() => undefined);
    audioContextRef.current = null;
  }

  function startVad(stream: MediaStream) {
    if (!vadEnabled) return;
    stopVad();
    const AudioCtx = window.AudioContext || window.webkitAudioContext;
    if (!AudioCtx) return;
    const ctx = new AudioCtx();
    audioContextRef.current = ctx;
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 1024;
    ctx.createMediaStreamSource(stream).connect(analyser);
    const data = new Uint8Array(analyser.fftSize);
    let heardVoice = false;
    let lastVoiceAt = Date.now();
    const started = Date.now();
    vadTimerRef.current = window.setInterval(() => {
      analyser.getByteTimeDomainData(data);
      let sum = 0;
      for (const value of data) {
        const centered = (value - 128) / 128;
        sum += centered * centered;
      }
      const rms = Math.sqrt(sum / data.length);
      if (rms > 0.035) {
        heardVoice = true;
        lastVoiceAt = Date.now();
      }
      if (heardVoice && Date.now() - started > 1400 && Date.now() - lastVoiceAt > 1100) {
        stopRecording();
      }
    }, 120);
  }

  async function startRecording() {
    if (!supported || recording || transcribing || busy) return;
    if (!active) onStart();
    onStopAudio();
    setError("");
    setLastTranscript("");
    const recordingToken = recordingTokenRef.current + 1;
    recordingTokenRef.current = recordingToken;
    discardRecordingRef.current = false;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
      });
      if (discardRecordingRef.current || recordingToken !== recordingTokenRef.current) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      streamRef.current = stream;
      chunksRef.current = [];
      const recorder = new MediaRecorder(stream);
      recorderRef.current = recorder;
      recorder.ondataavailable = (event) => {
        if (event.data?.size) chunksRef.current.push(event.data);
      };
      recorder.onstop = () => {
        const blob = new Blob(chunksRef.current, { type: recorder.mimeType || "audio/webm" });
        const shouldTranscribe = !discardRecordingRef.current && recordingToken === recordingTokenRef.current;
        chunksRef.current = [];
        recorderRef.current = null;
        stopVad();
        releaseStream();
        setRecording(false);
        if (shouldTranscribe && blob.size > 0) void transcribe(blob, recordingToken);
      };
      recorder.start();
      startVad(stream);
      setRecording(true);
    } catch (err) {
      stopVad();
      releaseStream();
      setRecording(false);
      if (!discardRecordingRef.current) setError(String(err));
    }
  }

  function stopRecording() {
    discardRecordingRef.current = false;
    const recorder = recorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      recorder.stop();
    } else {
      recorderRef.current = null;
      stopVad();
      releaseStream();
      setRecording(false);
    }
  }

  function cancelVoiceInput() {
    discardRecordingRef.current = true;
    transcriptionTokenRef.current += 1;
    chunksRef.current = [];
    const recorder = recorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      try {
        recorder.stop();
      } catch {
        recorderRef.current = null;
      }
    } else {
      recorderRef.current = null;
      stopVad();
      releaseStream();
      setRecording(false);
    }
    setTranscribing(false);
  }

  async function transcribe(blob: Blob, recordingToken: number) {
    const transcriptionToken = transcriptionTokenRef.current + 1;
    transcriptionTokenRef.current = transcriptionToken;
    setTranscribing(true);
    setError("");
    try {
      const status = await api.voiceStatus();
      if (transcriptionToken !== transcriptionTokenRef.current || recordingToken !== recordingTokenRef.current) return;
      if (!status.ready) throw new Error(status.reason || t("voice.call.sttNotReady"));
      const audio = Array.from(new Uint8Array(await blob.arrayBuffer()));
      const text = (await api.voiceTranscribe(audio, blob.type || "audio/webm")).trim();
      if (transcriptionToken !== transcriptionTokenRef.current || recordingToken !== recordingTokenRef.current) return;
      if (!text) return;
      setLastTranscript(text);
      await onTranscript(text);
    } catch (err) {
      if (transcriptionToken === transcriptionTokenRef.current) setError(String(err));
    } finally {
      if (transcriptionToken === transcriptionTokenRef.current) setTranscribing(false);
    }
  }

  function toggleRecording() {
    if (recording) stopRecording();
    else void startRecording();
  }

  function endCall() {
    cancelVoiceInput();
    onStopAudio();
    onEnd();
  }

  function closePanel() {
    cancelVoiceInput();
    onStopAudio();
    onClose();
  }

  if (!open) return null;

  const ttsReady = !!settings?.voice_enabled && settings.voice_tts_backend !== "none";

  return (
    <div className="border-b border-[#eceff3] bg-[#fbfcfd] px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className={`grid size-8 place-items-center rounded-md ${active ? "bg-[#111827] text-white" : "bg-white text-[#59616d] border border-[#e2e5ea]"}`}>
            <PhoneIcon size={16} />
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-[12px] font-semibold text-[#202124]">
              {t("voice.call.title")}
              <span className="font-mono text-[11px] text-[#7a8088]">{formatDuration(startedAt, now)}</span>
            </div>
            <div className="max-w-[48vw] truncate text-[11px] text-[#7a8088]">
              {recording
                ? t("voice.call.recording")
                : transcribing
                  ? t("voice.call.transcribing")
                  : ttsStatus.speaking
                    ? t("voice.speaking")
                    : t("voice.queue", { n: ttsStatus.queued })}
            </div>
          </div>
        </div>

        <div className="ml-auto flex items-center gap-1">
          <button
            type="button"
            className={`cf-press inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-[12px] font-medium transition ${
              recording ? "bg-[#fff1f1] text-[#b42318]" : "bg-[#111827] text-white hover:bg-[#2b3442]"
            } disabled:cursor-not-allowed disabled:opacity-50`}
            disabled={!supported || !ttsReady || transcribing || busy}
            onClick={toggleRecording}
          >
            {recording ? <StopIcon size={13} /> : <MicIcon size={14} />}
            {recording ? t("voice.call.stopTalk") : t("voice.call.talk")}
          </button>
          <button
            type="button"
            className={`grid h-8 w-8 place-items-center rounded-md transition ${
              muted ? "bg-[#fff1f1] text-[#b42318]" : "text-[#59616d] hover:bg-[#eef1f5]"
            }`}
            onClick={() => onMutedChange(!muted)}
            title={muted ? t("voice.call.unmute") : t("voice.call.mute")}
            aria-label={muted ? t("voice.call.unmute") : t("voice.call.mute")}
          >
            <VolumeIcon size={15} />
          </button>
          <label className="hidden h-8 items-center gap-1 rounded-md px-2 text-[11px] text-[#6f7782] md:flex">
            <input
              type="checkbox"
              className="size-3 accent-[#111827]"
              checked={vadEnabled}
              onChange={(event) => setVadEnabled(event.target.checked)}
            />
            VAD
          </label>
          {active ? (
            <button type="button" className="cf-press h-8 rounded-md px-3 text-[12px] font-medium text-[#59616d] hover:bg-[#eef1f5]" onClick={endCall}>
              {t("voice.call.end")}
            </button>
          ) : (
            <button type="button" className="cf-press h-8 rounded-md px-3 text-[12px] font-medium text-[#59616d] hover:bg-[#eef1f5]" onClick={onStart}>
              {t("voice.call.start")}
            </button>
          )}
          <button
            type="button"
            className="grid h-8 w-8 place-items-center rounded-md text-[#7a8088] hover:bg-[#eef1f5]"
            onClick={closePanel}
            aria-label={t("voice.call.close")}
            title={t("voice.call.close")}
          >
            <CloseIcon size={15} />
          </button>
        </div>
      </div>
      {(error || lastTranscript || ttsStatus.error) && (
        <div className="mt-2 truncate rounded-md border border-[#e2e5ea] bg-white px-3 py-2 text-[11px] text-[#6f7782]">
          {error || ttsStatus.error || lastTranscript}
        </div>
      )}
    </div>
  );
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}

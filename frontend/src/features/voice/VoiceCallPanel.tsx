import { useEffect, useRef, useState } from "react";
import * as api from "@/lib/api";
import type { Settings, VoiceStatus } from "@/lib/types";
import type { StreamingTtsStatus } from "@/lib/useStreamingTtsQueue";
import { recordedAudioSizeError } from "@/lib/voiceCapture";
import { CloseIcon, MicIcon, MicOffIcon, PhoneIcon, VolumeIcon, VolumeOffIcon } from "@/shared/components/Icons";
import { Select, type SelectOption } from "@/shared/components/Select";
import { useI18n } from "@/lib/i18n";

type Props = {
  open: boolean;
  active: boolean;
  muted: boolean;
  busy: boolean;
  startedAt: number | null;
  settings: Settings | null;
  ttsStatus: StreamingTtsStatus;
  characterName: string;
  avatarUrl?: string;
  onStart: () => void;
  onClose: () => void;
  onMutedChange: (muted: boolean) => void;
  onTranscript: (text: string) => Promise<boolean> | boolean;
  onStopAudio: () => void;
};

const WAVE_WEIGHTS = [0.42, 0.68, 0.9, 0.58, 1, 0.72, 0.48, 0.82, 0.55];
const VOICE_DEVICE_STORAGE_KEY = "demiurge.voiceInputDeviceId";

type MicDevice = { deviceId: string; label: string };

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

function VoiceWave({ level, listening, speaking }: { level: number; listening: boolean; speaking: boolean }) {
  return (
    <div className="voice-call-wave" aria-hidden="true">
      {WAVE_WEIGHTS.map((weight, index) => {
        const height = listening ? 6 + Math.round(Math.min(1, level) * weight * 42) : 8 + Math.round(weight * 8);
        return (
          <span
            key={index}
            className={`voice-call-wave-line ${speaking ? "is-speaking" : ""}`}
            style={{ height: `${height}px`, animationDelay: `${index * -90}ms` }}
          />
        );
      })}
    </div>
  );
}

export default function VoiceCallPanel({
  open,
  active,
  muted,
  busy,
  startedAt,
  settings,
  ttsStatus,
  characterName,
  avatarUrl,
  onStart,
  onClose,
  onMutedChange,
  onTranscript,
  onStopAudio,
}: Props) {
  const { t } = useI18n();
  const supported = recordingSupported();
  const [recording, setRecording] = useState(false);
  const [transcribing, setTranscribing] = useState(false);
  const [microphoneMuted, setMicrophoneMuted] = useState(true);
  const [listeningArmed, setListeningArmed] = useState(false);
  const [inputLevel, setInputLevel] = useState(0);
  const [devices, setDevices] = useState<MicDevice[]>([]);
  const [selectedDeviceId, setSelectedDeviceId] = useState(() => {
    if (typeof localStorage === "undefined") return "";
    return localStorage.getItem(VOICE_DEVICE_STORAGE_KEY) ?? "";
  });
  const [error, setError] = useState("");
  const [lastTranscript, setLastTranscript] = useState("");
  const [now, setNow] = useState(Date.now());
  const [readiness, setReadiness] = useState<VoiceStatus | null>(null);

  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const vadTimerRef = useRef<number | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const recordingTokenRef = useRef(0);
  const discardRecordingRef = useRef(false);
  const transcriptionTokenRef = useRef(0);

  const sttReady = readiness?.enabled === true && readiness.ready;
  const ttsReady = readiness?.enabled === true && readiness.tts_ready;

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setReadiness(null);
    void api
      .voiceStatus()
      .then((status) => {
        if (!cancelled) setReadiness(status);
      })
      .catch((reason) => {
        if (!cancelled) setError(String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [
    open,
    settings?.voice_enabled,
    settings?.voice_stt_backend,
    settings?.voice_tts_backend,
    settings?.voice_id,
    settings?.media_base_url,
    settings?.api_key,
    settings?.media_api_key,
  ]);

  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active]);

  useEffect(() => {
    if (!open || !active) return;
    const onHotkey = () => toggleMicrophone();
    window.addEventListener("demiurge-voice-hotkey", onHotkey);
    return () => window.removeEventListener("demiurge-voice-hotkey", onHotkey);
    // State is intentionally captured so the hotkey mirrors the visible control.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, active, microphoneMuted, recording, transcribing]);

  useEffect(() => {
    if (!open || !supported || !navigator.mediaDevices.enumerateDevices) return;
    void refreshDevices();
    const onDeviceChange = () => void refreshDevices();
    navigator.mediaDevices.addEventListener?.("devicechange", onDeviceChange);
    return () => navigator.mediaDevices.removeEventListener?.("devicechange", onDeviceChange);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, supported]);

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
    if (open) return;
    cancelVoiceInput();
    setMicrophoneMuted(true);
    setListeningArmed(false);
    setError("");
    setLastTranscript("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    if (!ttsStatus.speaking || !recording) return;
    cancelVoiceInput();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ttsStatus.speaking]);

  useEffect(() => {
    if (
      !open ||
      !active ||
      !listeningArmed ||
      microphoneMuted ||
      recording ||
      transcribing ||
      busy ||
      ttsStatus.speaking ||
      !sttReady ||
      !supported
    ) {
      return;
    }
    const timer = window.setTimeout(() => void startRecording(), 260);
    return () => window.clearTimeout(timer);
    // Re-enter listening when the assistant finishes and the call becomes idle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, active, listeningArmed, microphoneMuted, recording, transcribing, busy, ttsStatus.speaking, sttReady, supported, selectedDeviceId]);

  function releaseStream() {
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
  }

  async function refreshDevices() {
    if (!supported || !navigator.mediaDevices.enumerateDevices) return;
    try {
      const all = await navigator.mediaDevices.enumerateDevices();
      setDevices(
        all
          .filter((device) => device.kind === "audioinput")
          .map((device, index) => ({
            deviceId: device.deviceId,
            label: device.label || t("composer.voiceMicN", { n: index + 1 }),
          })),
      );
    } catch {
      // Keep the last known list when enumeration is temporarily unavailable.
    }
  }

  function stopVad() {
    if (vadTimerRef.current) {
      window.clearInterval(vadTimerRef.current);
      vadTimerRef.current = null;
    }
    void audioContextRef.current?.close().catch(() => undefined);
    audioContextRef.current = null;
    setInputLevel(0);
  }

  function startVad(stream: MediaStream) {
    stopVad();
    const AudioCtx = window.AudioContext || window.webkitAudioContext;
    if (!AudioCtx) return;
    const ctx = new AudioCtx();
    audioContextRef.current = ctx;
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 1024;
    analyser.smoothingTimeConstant = 0.72;
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
      setInputLevel(Math.min(1, rms * 9));
      if (rms > 0.035) {
        heardVoice = true;
        lastVoiceAt = Date.now();
      }
      if (heardVoice && Date.now() - started > 1400 && Date.now() - lastVoiceAt > 1100) {
        stopRecording();
      }
    }, 70);
  }

  async function startRecording(force = false, deviceId = selectedDeviceId) {
    if (
      !supported ||
      recording ||
      transcribing ||
      busy ||
      (!force && microphoneMuted) ||
      (!force && ttsStatus.speaking)
    ) {
      return;
    }
    if (!settings?.voice_enabled) {
      setMicrophoneMuted(true);
      setListeningArmed(false);
      setError(readiness?.reason || t("voice.call.sttNotReady"));
      return;
    }
    const recordingToken = recordingTokenRef.current + 1;
    recordingTokenRef.current = recordingToken;
    discardRecordingRef.current = false;
    let status: VoiceStatus;
    try {
      status = await api.voiceStatus();
      if (discardRecordingRef.current || recordingToken !== recordingTokenRef.current) return;
      setReadiness(status);
    } catch (err) {
      if (discardRecordingRef.current || recordingToken !== recordingTokenRef.current) return;
      setMicrophoneMuted(true);
      setListeningArmed(false);
      setError(String(err));
      return;
    }
    if (!status.enabled || !status.ready) {
      setMicrophoneMuted(true);
      setListeningArmed(false);
      setError(status.reason || t("voice.call.sttNotReady"));
      return;
    }
    if (!active) onStart();
    setError("");
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          deviceId: deviceId ? { exact: deviceId } : undefined,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
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
      void refreshDevices();
    } catch (err) {
      stopVad();
      releaseStream();
      setRecording(false);
      if (!discardRecordingRef.current) {
        setMicrophoneMuted(true);
        setListeningArmed(false);
        setError(String(err));
      }
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
    recordingTokenRef.current += 1;
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
      const sizeError = recordedAudioSizeError(blob.size);
      if (sizeError) throw new Error(sizeError);
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

  function toggleMicrophone() {
    if (microphoneMuted) {
      onStopAudio();
      setMicrophoneMuted(false);
      setListeningArmed(true);
      setError("");
      if (!active) onStart();
      void startRecording(true);
      return;
    }
    setMicrophoneMuted(true);
    setListeningArmed(false);
    cancelVoiceInput();
  }

  function selectMicrophone(deviceId: string) {
    cancelVoiceInput();
    setSelectedDeviceId(deviceId);
    if (typeof localStorage !== "undefined") localStorage.setItem(VOICE_DEVICE_STORAGE_KEY, deviceId);
  }

  function endCall() {
    cancelVoiceInput();
    onStopAudio();
    onClose();
  }

  if (!open) return null;

  const statusText = microphoneMuted
    ? t("voice.call.micOff")
    : recording
      ? t("voice.call.recording")
      : transcribing
        ? t("voice.call.transcribing")
        : ttsStatus.speaking
          ? t("voice.call.speaking")
          : busy
            ? t("voice.call.thinking")
            : t("voice.call.connected");
  const microphoneOptions: SelectOption[] = [
    { value: "", label: t("voice.call.defaultMicrophone"), icon: <MicIcon size={14} /> },
    ...devices
      .filter((device) => device.deviceId)
      .map((device) => ({ value: device.deviceId, label: device.label, icon: <MicIcon size={14} /> })),
  ];

  return (
    <div className="voice-call-backdrop fixed inset-0 z-[90] grid place-items-center bg-[#111827]/30 p-4">
      <section
        className="voice-call-surface flex h-[min(560px,calc(100vh-32px))] w-[min(390px,calc(100vw-32px))] flex-col overflow-hidden rounded-lg border border-[#cfd4dc] bg-[#f8f9fb] shadow-[0_18px_48px_rgba(15,23,42,0.24)]"
        role="dialog"
        aria-modal="true"
        aria-label={t("voice.call.title")}
      >
        <header className="flex h-11 shrink-0 items-center border-b border-[#dde1e7] bg-[#f2f4f7] px-3">
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <PhoneIcon size={15} className="shrink-0 text-[#4f5966]" />
            <span className="md-type-title-small truncate font-semibold text-[#252a31]">{t("voice.call.title")}</span>
            <span className="size-1.5 shrink-0 rounded-full bg-[#279562]" aria-hidden="true" />
            <span className="md-type-label-small font-mono tabular-nums text-[#737b86]">
              {formatDuration(startedAt, now)}
            </span>
          </div>
          <button
            type="button"
            className="md-icon-button cf-press grid size-7 place-items-center rounded text-[#66707d] transition hover:bg-[#e3e7ec] hover:text-[#202124]"
            onClick={endCall}
            aria-label={t("voice.call.close")}
            title={t("voice.call.close")}
          >
            <CloseIcon size={16} />
          </button>
        </header>

        <div className="flex min-h-0 flex-1 flex-col items-center px-6 pb-4 pt-7 text-center">
          <div className="relative mt-1">
            <div className={`voice-call-avatar-ring ${recording || ttsStatus.speaking ? "is-active" : ""}`} />
            <img
              src={avatarUrl || "/demiurge.png"}
              alt=""
              className="relative size-28 rounded-full border-2 border-white bg-white object-cover shadow-[0_5px_16px_rgba(15,23,42,0.15)]"
            />
            <span className="absolute bottom-1.5 right-1.5 size-3.5 rounded-full border-2 border-white bg-[#279562]" />
          </div>

          <h2 className="md-type-title-large mt-4 max-w-full truncate font-semibold text-[#202124]">{characterName}</h2>
          <div className="md-type-body-small mt-1 text-[#737b86]">{statusText}</div>

          <div className="mt-6 grid h-20 w-full place-items-center border-y border-[#e1e5ea] bg-[#f4f6f8]">
            <VoiceWave level={inputLevel} listening={recording} speaking={ttsStatus.speaking} />
          </div>

          <div className="md-type-body-small mt-3 min-h-10 w-full px-2">
            {error || ttsStatus.error ? (
              <p className="line-clamp-2 text-[#b42318]">{error || ttsStatus.error}</p>
            ) : lastTranscript ? (
              <p className="line-clamp-2 text-[#66707d]">“{lastTranscript}”</p>
            ) : !supported ? (
              <p className="text-[#b42318]">{t("voice.call.unsupported")}</p>
            ) : readiness && !sttReady ? (
              <p className="text-[#8a6215]">{readiness.reason || t("voice.call.sttNotReady")}</p>
            ) : !ttsReady ? (
              <p className="text-[#8a6215]">{readiness?.tts_reason || t("voice.call.ttsNotReady")}</p>
            ) : null}
          </div>
        </div>

        <div className="shrink-0 border-t border-[#d9dee5] bg-white px-4 pb-4 pt-3">
          <div className="mb-3 flex items-center gap-3">
            <span className="md-type-label-medium w-[68px] shrink-0 text-[#66707d]">{t("composer.voicePick")}</span>
            <div className="min-w-0 flex-1">
              <Select
                value={selectedDeviceId}
                options={microphoneOptions}
                onChange={selectMicrophone}
                placeholder={t("voice.call.defaultMicrophone")}
                direction="up"
                disabled={!supported}
                triggerClassName="md-type-label-medium flex h-10 w-full min-w-0 items-center gap-1.5 rounded-md border border-[#cfd5dd] bg-[#f8f9fb] px-2.5 text-[#303640] outline-none transition hover:border-[#aeb6c1] hover:bg-white focus:border-[#8993a1] focus:shadow-[0_0_0_2px_rgba(17,24,39,0.08)] disabled:cursor-not-allowed disabled:opacity-50"
              />
            </div>
          </div>

          <div className="grid grid-cols-3 gap-2">
            <button
              type="button"
              className={`voice-call-tool cf-press ${microphoneMuted ? "" : "is-active"}`}
              onClick={toggleMicrophone}
              disabled={!supported || !sttReady}
              aria-pressed={!microphoneMuted}
              aria-label={microphoneMuted ? t("voice.call.micOn") : t("voice.call.micOffAction")}
              title={microphoneMuted ? t("voice.call.micOn") : t("voice.call.micOffAction")}
            >
              {microphoneMuted ? <MicOffIcon size={18} /> : <MicIcon size={18} />}
              <span>{t("voice.call.microphone")}</span>
            </button>

            <button
              type="button"
              className={`voice-call-tool cf-press ${muted ? "" : "is-active"}`}
              onClick={() => onMutedChange(!muted)}
              disabled={!ttsReady}
              aria-pressed={!muted}
              aria-label={muted ? t("voice.call.speakerOn") : t("voice.call.speakerOff")}
              title={muted ? t("voice.call.speakerOn") : t("voice.call.speakerOff")}
            >
              {muted ? <VolumeOffIcon size={18} /> : <VolumeIcon size={18} />}
              <span>{t("voice.call.speaker")}</span>
            </button>

            <button
              type="button"
              className="voice-call-tool is-danger cf-press"
              onClick={endCall}
              aria-label={t("voice.call.end")}
              title={t("voice.call.end")}
            >
              <PhoneIcon size={18} className="rotate-[135deg]" />
              <span>{t("voice.call.end")}</span>
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}

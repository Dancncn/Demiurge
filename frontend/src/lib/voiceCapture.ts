export const MAX_STT_AUDIO_BYTES = 25 * 1024 * 1024;

export function recordedAudioSizeError(size: number) {
  if (Number.isFinite(size) && size >= 0 && size <= MAX_STT_AUDIO_BYTES) return "";
  return `Recorded audio is too large (maximum ${Math.floor(MAX_STT_AUDIO_BYTES / 1024 / 1024)} MB).`;
}

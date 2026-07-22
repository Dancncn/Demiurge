export interface StoppableAudio {
  currentTime: number;
  pause: () => void;
  removeAttribute: (name: string) => void;
  load: () => void;
}

export function haltAudioPlayback(audio: StoppableAudio) {
  try {
    audio.pause();
  } catch {
    // A detached media element can reject pause; source release still matters.
  }
  try {
    audio.currentTime = 0;
  } catch {
    // Streaming media may not be seekable yet.
  }
  try {
    audio.removeAttribute("src");
    audio.load();
  } catch {
    // The element may already have been detached by the WebView.
  }
}

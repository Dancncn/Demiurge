import assert from "node:assert/strict";
import test from "node:test";
import { MAX_STT_AUDIO_BYTES, recordedAudioSizeError } from "../src/lib/voiceCapture.ts";

test("recorded audio is rejected above the backend STT limit", () => {
  assert.equal(recordedAudioSizeError(MAX_STT_AUDIO_BYTES), "");
  assert.match(recordedAudioSizeError(MAX_STT_AUDIO_BYTES + 1), /25 MB/);
});

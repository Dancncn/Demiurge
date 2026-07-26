import assert from "node:assert/strict";
import test from "node:test";
import { haltAudioPlayback } from "../src/lib/audioPlayback.ts";

test("haltAudioPlayback synchronously pauses and releases the source", () => {
  const calls: string[] = [];
  const audio = {
    currentTime: 12,
    pause: () => calls.push("pause"),
    removeAttribute: (name: string) => calls.push(`remove:${name}`),
    load: () => calls.push("load"),
  };

  haltAudioPlayback(audio);

  assert.equal(audio.currentTime, 0);
  assert.deepEqual(calls, ["pause", "remove:src", "load"]);
});

test("haltAudioPlayback tolerates media elements that reject seeking or load", () => {
  const audio = {
    get currentTime() {
      return 0;
    },
    set currentTime(_value: number) {
      throw new Error("not seekable");
    },
    pause: () => undefined,
    removeAttribute: () => undefined,
    load: () => {
      throw new Error("detached");
    },
  };

  assert.doesNotThrow(() => haltAudioPlayback(audio));
});

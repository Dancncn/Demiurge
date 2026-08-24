import assert from "node:assert/strict";
import test from "node:test";

import { multimodalImages, type ProcessedAttachment } from "../src/lib/fileProcessing.ts";

test("multimodalImages forwards only ready native image payloads", () => {
  const attachments: ProcessedAttachment[] = [
    {
      id: "image",
      name: "sample.png",
      size: 3,
      mime: "image/png",
      kind: "image",
      status: "ready",
      imageInput: { mime_type: "image/png", data: "YWJj", name: "sample.png" },
    },
    {
      id: "text",
      name: "notes.txt",
      size: 4,
      mime: "text/plain",
      kind: "text",
      status: "ready",
      content: "note",
    },
    {
      id: "failed",
      name: "bad.png",
      size: 0,
      mime: "image/png",
      kind: "image",
      status: "error",
      error: "empty",
      imageInput: { mime_type: "image/png", data: "", name: "bad.png" },
    },
  ];

  assert.deepEqual(multimodalImages(attachments), [
    { mime_type: "image/png", data: "YWJj", name: "sample.png" },
  ]);
});

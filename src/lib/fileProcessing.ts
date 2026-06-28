import JSZip from "jszip";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.mjs?url";

export type AttachmentKind = "text" | "image" | "pdf" | "document" | "spreadsheet" | "presentation" | "unsupported";

export type AttachmentStatus = "ready" | "error";

export interface ProcessedAttachment {
  id: string;
  name: string;
  size: number;
  mime: string;
  kind: AttachmentKind;
  status: AttachmentStatus;
  content?: string;
  previewUrl?: string;
  note?: string;
  error?: string;
  truncated?: boolean;
}

export type ImageTextExtractor = (bytes: number[]) => Promise<string>;

type PdfJsModule = {
  GlobalWorkerOptions: { workerSrc: string };
  getDocument: (src: { data: Uint8Array }) => {
    promise: Promise<{
      numPages: number;
      getPage: (pageNumber: number) => Promise<{
        getTextContent: () => Promise<{ items: Array<{ str?: string }> }>;
      }>;
    }>;
  };
};

const MAX_FILE_CHARS = 28_000;
const MAX_PDF_PAGES = 80;

const TEXT_EXTENSIONS = new Set([
  "txt",
  "md",
  "markdown",
  "csv",
  "tsv",
  "json",
  "jsonl",
  "yaml",
  "yml",
  "toml",
  "xml",
  "html",
  "htm",
  "css",
  "scss",
  "js",
  "jsx",
  "ts",
  "tsx",
  "mjs",
  "cjs",
  "py",
  "rs",
  "go",
  "java",
  "kt",
  "c",
  "cpp",
  "h",
  "hpp",
  "cs",
  "php",
  "rb",
  "swift",
  "sql",
  "sh",
  "ps1",
  "bat",
  "log",
]);

export const attachmentAccept =
  "text/*,image/*,.md,.markdown,.csv,.tsv,.json,.jsonl,.yaml,.yml,.toml,.xml,.html,.css,.js,.jsx,.ts,.tsx,.py,.rs,.go,.java,.sql,.sh,.ps1,.log,.pdf,.docx,.pptx,.xlsx,.xlsm";

export function formatAttachmentSize(n: number) {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = n;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

export function attachmentKindLabel(kind: AttachmentKind) {
  if (kind === "pdf") return "PDF";
  if (kind === "document") return "Document";
  if (kind === "spreadsheet") return "Spreadsheet";
  if (kind === "presentation") return "Presentation";
  if (kind === "image") return "Image";
  if (kind === "text") return "Text";
  return "File";
}

export function releaseAttachment(attachment: ProcessedAttachment) {
  if (attachment.previewUrl?.startsWith("blob:")) {
    URL.revokeObjectURL(attachment.previewUrl);
  }
}

export async function processFiles(
  files: File[],
  options: { extractImageText?: ImageTextExtractor } = {},
): Promise<ProcessedAttachment[]> {
  const picked = files.slice(0, 8);
  return Promise.all(picked.map((file) => processFile(file, options)));
}

export function buildAttachmentPrompt(attachments: ProcessedAttachment[]) {
  const usable = attachments.filter((attachment) => attachment.status === "ready");
  if (usable.length === 0) return "";

  const blocks = usable.map((attachment, index) => {
    const header = [
      `### ${index + 1}. ${attachment.name}`,
      `- Kind: ${attachmentKindLabel(attachment.kind)}`,
      `- MIME: ${attachment.mime || "unknown"}`,
      `- Size: ${formatAttachmentSize(attachment.size)}`,
    ];
    if (attachment.note) header.push(`- Note: ${attachment.note}`);
    if (!attachment.content) return header.join("\n");
    return `${header.join("\n")}\n\n<file name="${escapeAttribute(attachment.name)}">\n${attachment.content}\n</file>`;
  });

  return `\n\nAttached files processed by Demiurge:\n${blocks.join("\n\n")}`;
}

async function processFile(
  file: File,
  options: { extractImageText?: ImageTextExtractor },
): Promise<ProcessedAttachment> {
  const kind = detectKind(file);
  const base = {
    id: `${Date.now()}-${Math.random().toString(36).slice(2)}`,
    name: file.name,
    size: file.size,
    mime: file.type,
    kind,
  };

  try {
    if (kind === "text") {
      return {
        ...base,
        status: "ready",
        content: clip(await file.text()),
      };
    }
    if (kind === "image") {
      const previewUrl = URL.createObjectURL(file);
      if (options.extractImageText) {
        try {
          const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
          const text = await options.extractImageText(bytes);
          return {
            ...base,
            status: "ready",
            previewUrl,
            content: clip(text),
            note: "OCR text extracted from image.",
          };
        } catch (error) {
          return {
            ...base,
            status: "ready",
            previewUrl,
            note: `Image preview is available. OCR did not run: ${
              error instanceof Error ? error.message : String(error)
            }`,
          };
        }
      }
      return {
        ...base,
        status: "ready",
        previewUrl,
        note: "Image preview is available. OCR integration is not configured.",
      };
    }
    if (kind === "pdf") {
      return {
        ...base,
        status: "ready",
        content: clip(await extractPdfText(file)),
      };
    }
    if (kind === "document") {
      return {
        ...base,
        status: "ready",
        content: clip(await extractDocxText(file)),
      };
    }
    if (kind === "presentation") {
      return {
        ...base,
        status: "ready",
        content: clip(await extractPptxText(file)),
      };
    }
    if (kind === "spreadsheet") {
      return {
        ...base,
        status: "ready",
        content: clip(await extractXlsxText(file)),
      };
    }
    return {
      ...base,
      status: "error",
      error: "Unsupported file type. Use text, images, PDF, DOCX, PPTX, XLSX or XLSM.",
    };
  } catch (error) {
    return {
      ...base,
      status: "error",
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function detectKind(file: File): AttachmentKind {
  const ext = extension(file.name);
  if (file.type.startsWith("image/")) return "image";
  if (file.type.startsWith("text/") || TEXT_EXTENSIONS.has(ext)) return "text";
  if (ext === "pdf" || file.type === "application/pdf") return "pdf";
  if (ext === "docx") return "document";
  if (ext === "pptx") return "presentation";
  if (ext === "xlsx" || ext === "xlsm") return "spreadsheet";
  return "unsupported";
}

function extension(name: string) {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

function clip(content: string) {
  const normalized = content.trim();
  if (normalized.length <= MAX_FILE_CHARS) return normalized;
  return `${normalized.slice(0, MAX_FILE_CHARS)}\n\n[Attachment text truncated at ${MAX_FILE_CHARS} characters.]`;
}

async function extractPdfText(file: File) {
  const pdfjs = (await import("pdfjs-dist/build/pdf.mjs")) as unknown as PdfJsModule;
  pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
  const pdf = await pdfjs.getDocument({ data: new Uint8Array(await file.arrayBuffer()) }).promise;
  const pageCount = Math.min(pdf.numPages, MAX_PDF_PAGES);
  const pages: string[] = [];
  for (let i = 1; i <= pageCount; i += 1) {
    const page = await pdf.getPage(i);
    const text = await page.getTextContent();
    const body = text.items.map((item) => item.str ?? "").join(" ").trim();
    if (body) pages.push(`Page ${i}\n${body}`);
  }
  if (pdf.numPages > MAX_PDF_PAGES) {
    pages.push(`[Only the first ${MAX_PDF_PAGES} of ${pdf.numPages} pages were extracted.]`);
  }
  return pages.join("\n\n");
}

async function extractDocxText(file: File) {
  const zip = await JSZip.loadAsync(await file.arrayBuffer());
  const xml = await zip.file("word/document.xml")?.async("text");
  if (!xml) throw new Error("DOCX document.xml not found.");
  return paragraphsFromXml(xml).join("\n");
}

async function extractPptxText(file: File) {
  const zip = await JSZip.loadAsync(await file.arrayBuffer());
  const slidePaths = Object.keys(zip.files)
    .filter((name) => /^ppt\/slides\/slide\d+\.xml$/i.test(name))
    .sort((a, b) => Number(a.match(/\d+/)?.[0] ?? 0) - Number(b.match(/\d+/)?.[0] ?? 0));

  const slides: string[] = [];
  for (const [index, path] of slidePaths.entries()) {
    const xml = await zip.file(path)?.async("text");
    if (!xml) continue;
    const text = textRunsFromXml(xml).join("\n").trim();
    if (text) slides.push(`Slide ${index + 1}\n${text}`);
  }
  return slides.join("\n\n");
}

async function extractXlsxText(file: File) {
  const zip = await JSZip.loadAsync(await file.arrayBuffer());
  const shared = await readSharedStrings(zip);
  const sheetPaths = Object.keys(zip.files)
    .filter((name) => /^xl\/worksheets\/sheet\d+\.xml$/i.test(name))
    .sort((a, b) => Number(a.match(/\d+/)?.[0] ?? 0) - Number(b.match(/\d+/)?.[0] ?? 0));

  const sheets: string[] = [];
  for (const [index, path] of sheetPaths.entries()) {
    const xml = await zip.file(path)?.async("text");
    if (!xml) continue;
    const rows = rowsFromWorksheetXml(xml, shared);
    if (rows.length) sheets.push(`Sheet ${index + 1}\n${rows.join("\n")}`);
  }
  return sheets.join("\n\n");
}

async function readSharedStrings(zip: JSZip) {
  const xml = await zip.file("xl/sharedStrings.xml")?.async("text");
  return xml ? textRunsFromXml(xml) : [];
}

function paragraphsFromXml(xml: string) {
  const doc = parseXml(xml);
  const paragraphs: string[] = [];
  for (const node of Array.from(doc.getElementsByTagName("*"))) {
    if (node.localName !== "p") continue;
    const text = textRunsFromElement(node).join("").trim();
    if (text) paragraphs.push(text);
  }
  return paragraphs;
}

function textRunsFromXml(xml: string) {
  return textRunsFromElement(parseXml(xml).documentElement).map((value) => value.trim()).filter(Boolean);
}

function textRunsFromElement(root: Element) {
  const values: string[] = [];
  for (const node of Array.from(root.getElementsByTagName("*"))) {
    if (node.localName === "t" && node.textContent) values.push(node.textContent);
  }
  return values;
}

function rowsFromWorksheetXml(xml: string, shared: string[]) {
  const doc = parseXml(xml);
  const rows: string[] = [];
  for (const row of Array.from(doc.getElementsByTagName("*")).filter((node) => node.localName === "row")) {
    const cells: string[] = [];
    for (const cell of Array.from(row.children).filter((node) => node.localName === "c")) {
      const type = cell.getAttribute("t");
      const value = firstChildText(cell, "v");
      if (type === "s") {
        cells.push(shared[Number(value)] ?? "");
      } else if (type === "inlineStr") {
        cells.push(textRunsFromElement(cell).join(""));
      } else {
        cells.push(value);
      }
    }
    const line = cells.join("\t").trim();
    if (line) rows.push(line);
  }
  return rows;
}

function firstChildText(root: Element, localName: string) {
  const child = Array.from(root.getElementsByTagName("*")).find((node) => node.localName === localName);
  return child?.textContent ?? "";
}

function parseXml(xml: string) {
  const doc = new DOMParser().parseFromString(xml, "application/xml");
  const errorNode = doc.getElementsByTagName("parsererror")[0];
  if (errorNode) throw new Error(errorNode.textContent || "Invalid XML in Office document.");
  return doc;
}

function escapeAttribute(value: string) {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

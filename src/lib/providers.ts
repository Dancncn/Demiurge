import type { ProviderKind, ReasoningEffort } from "./types";

export type ProviderOption = {
  value: ProviderKind;
  label: string;
  short: string;
  baseUrl: string;
  model: string;
  help: string;
  /** Suggested models for quick selection; users can still type any model name. */
  models: string[];
};

export const PROVIDER_OPTIONS: ProviderOption[] = [
  {
    value: "dashscope",
    label: "阿里云百炼 (DashScope)",
    short: "BL",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus-latest",
    help: "Aliyun Bailian / DashScope OpenAI-compatible endpoint. Media uses native DashScope APIs below.",
    models: ["qwen3-max", "qwen-plus-latest", "qwen-flash", "qwen3-coder-plus", "qwen-max-latest", "qwen-long"],
  },
  {
    value: "deepseek",
    label: "DeepSeek",
    short: "DS",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-v4-pro",
    help: "DeepSeek official OpenAI-compatible endpoint. deepseek-chat / deepseek-reasoner are stable aliases to the latest.",
    models: ["deepseek-v4-pro", "deepseek-v4-flash", "deepseek-reasoner", "deepseek-chat"],
  },
  {
    value: "openai",
    label: "ChatGPT / OpenAI",
    short: "AI",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-5.5",
    help: "OpenAI chat completions endpoint.",
    models: ["gpt-5.5", "gpt-5.5-pro", "gpt-5.4", "gpt-5.4-mini", "gpt-5.4-nano"],
  },
  {
    value: "openrouter",
    label: "OpenRouter",
    short: "OR",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "anthropic/claude-opus-4.8",
    help: "OpenRouter model gateway. Models use the vendor/model form (verified against the live OpenRouter catalog).",
    models: [
      "anthropic/claude-opus-4.8",
      "openai/gpt-5.5",
      "google/gemini-3.5-flash",
      "deepseek/deepseek-v4-pro",
      "x-ai/grok-4.3",
      "z-ai/glm-5.2",
    ],
  },
  {
    value: "anthropic",
    label: "Anthropic",
    short: "AN",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-6",
    help: "Claude API. The key is stored in the system credential manager.",
    models: ["claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5", "claude-fable-5", "claude-opus-4-7"],
  },
  {
    value: "gemini",
    label: "Google Gemini",
    short: "GE",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-3-pro-preview",
    help: "Google AI Studio compatible Gemini endpoint.",
    models: ["gemini-3-pro-preview", "gemini-3.5-flash", "gemini-3-flash-preview", "gemini-2.5-pro", "gemini-2.5-flash"],
  },
  {
    value: "glm",
    label: "智谱 GLM",
    short: "GL",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4.6",
    help: "Zhipu AI OpenAI-compatible endpoint.",
    models: ["glm-4.6", "glm-5", "glm-4.5", "glm-4.5-air", "glm-4.5-flash"],
  },
  {
    value: "minimax",
    label: "MiniMax",
    short: "MM",
    baseUrl: "https://api.minimax.chat/v1",
    model: "MiniMax-M2",
    help: "MiniMax OpenAI-compatible endpoint.",
    models: ["MiniMax-M3", "MiniMax-M2", "MiniMax-M1"],
  },
  {
    value: "xai",
    label: "xAI Grok",
    short: "XAI",
    baseUrl: "https://api.x.ai/v1",
    model: "grok-4-fast",
    help: "xAI Grok OpenAI-compatible endpoint. Use a standard xAI API key.",
    models: ["grok-4", "grok-4-fast", "grok-4-fast-reasoning", "grok-3", "grok-3-mini"],
  },
  {
    value: "groq",
    label: "Groq",
    short: "GQ",
    baseUrl: "https://api.groq.com/openai/v1",
    model: "llama-3.3-70b-versatile",
    help: "Groq LPU ultra-fast inference of open models. Model ids may carry an org prefix (e.g. moonshotai/...).",
    models: [
      "llama-3.3-70b-versatile",
      "moonshotai/kimi-k2-instruct-0905",
      "openai/gpt-oss-120b",
      "qwen/qwen3-32b",
      "llama-3.1-8b-instant",
    ],
  },
  {
    value: "mistral",
    label: "Mistral AI",
    short: "MI",
    baseUrl: "https://api.mistral.ai/v1",
    model: "mistral-large-latest",
    help: "Mistral AI OpenAI-compatible endpoint. The -latest aliases track the newest snapshot.",
    models: ["mistral-large-latest", "mistral-medium-latest", "mistral-small-latest", "magistral-medium-latest", "codestral-latest"],
  },
  {
    value: "moonshot",
    label: "Moonshot / Kimi",
    short: "KM",
    baseUrl: "https://api.moonshot.cn/v1",
    model: "kimi-k2-0905-preview",
    help: "Moonshot / Kimi OpenAI-compatible endpoint (api.moonshot.cn for CN, api.moonshot.ai for global).",
    models: ["kimi-k2-0905-preview", "kimi-latest", "moonshot-v1-128k", "moonshot-v1-32k", "moonshot-v1-8k"],
  },
  {
    value: "perplexity",
    label: "Perplexity",
    short: "PX",
    baseUrl: "https://api.perplexity.ai",
    model: "sonar",
    help: "Perplexity Sonar (search-augmented). Note: the base URL has no /v1 segment.",
    models: ["sonar", "sonar-pro", "sonar-reasoning", "sonar-reasoning-pro", "sonar-deep-research"],
  },
  {
    value: "doubao",
    label: "火山方舟 / 豆包 (Doubao)",
    short: "DB",
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    model: "doubao-seed-1.6",
    help: "ByteDance Volcengine Ark / Doubao. May require an endpoint id (ep-xxxx) or a date-suffixed model id (e.g. doubao-seed-1-6-251015).",
    models: ["doubao-seed-1.6", "doubao-seed-1.6-flash", "doubao-1.5-pro-32k", "doubao-1.5-pro-256k", "deepseek-v3"],
  },
  {
    value: "hunyuan",
    label: "腾讯混元 (Hunyuan)",
    short: "HY",
    baseUrl: "https://api.hunyuan.cloud.tencent.com/v1",
    model: "hunyuan-turbos-latest",
    help: "Tencent Hunyuan OpenAI-compatible endpoint. Uses a dedicated Hunyuan API key.",
    models: ["hunyuan-turbos-latest", "hunyuan-t1-latest", "hunyuan-large", "hunyuan-standard", "hunyuan-pro"],
  },
  {
    value: "stepfun",
    label: "阶跃星辰 (StepFun)",
    short: "ST",
    baseUrl: "https://api.stepfun.com/v1",
    model: "step-2-16k",
    help: "StepFun OpenAI-compatible endpoint. Confirm exact model ids against the StepFun model overview.",
    models: ["step-2-16k", "step-2-mini", "step-1-32k", "step-1-8k", "step-1v-8k"],
  },
  {
    value: "custom",
    label: "Custom Provider",
    short: "CU",
    baseUrl: "",
    model: "",
    help: "Any OpenAI-compatible endpoint.",
    models: [],
  },
  {
    value: "open_ai_compatible",
    label: "OpenAI Compatible",
    short: "OA",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
    help: "Generic OpenAI-compatible profile for self-hosted gateways.",
    models: ["deepseek-chat", "qwen-plus-latest", "gpt-5.5"],
  },
  {
    value: "local",
    label: "Local (Ollama / LM Studio)",
    short: "LO",
    baseUrl: "http://localhost:11434/v1",
    model: "qwen3",
    help: "Ollama, LM Studio, vLLM or any local OpenAI-compatible service.",
    models: ["qwen3", "llama3.3", "deepseek-r1", "gemma3", "phi4"],
  },
];

/** Provider keys that ship a logo under public/providers/<key>.svg (icons from @lobehub/icons-static-svg, MIT). */
export const PROVIDER_ICON_SET = new Set<ProviderKind>([
  "deepseek",
  "dashscope",
  "openai",
  "openrouter",
  "anthropic",
  "gemini",
  "glm",
  "minimax",
  "xai",
  "groq",
  "mistral",
  "moonshot",
  "perplexity",
  "doubao",
  "hunyuan",
  "stepfun",
  "open_ai_compatible",
  "local",
]);

export function findProvider(value: ProviderKind): ProviderOption {
  return PROVIDER_OPTIONS.find((p) => p.value === value) ?? PROVIDER_OPTIONS[0];
}

/**
 * Best-effort context window (max input tokens) for known model ids.
 * Used to auto-size the input budget so users don't have to guess a number.
 * Lookups are case-insensitive and fall back to provider/family heuristics.
 */
const MODEL_CONTEXT_WINDOWS: Record<string, number> = {
  // DashScope / Qwen
  "qwen3-max": 262_144,
  "qwen-max-latest": 131_072,
  "qwen-max": 131_072,
  "qwen-plus-latest": 131_072,
  "qwen-plus": 131_072,
  "qwen-flash": 1_000_000,
  "qwen-turbo-latest": 1_000_000,
  "qwen-turbo": 1_000_000,
  "qwen3-coder-plus": 1_000_000,
  "qwen-long": 10_000_000,
  // DeepSeek (per OpenRouter: V4 series 1M; chat/reasoner aliases ~128K)
  "deepseek-chat": 131_072,
  "deepseek-reasoner": 131_072,
  "deepseek-v4-pro": 1_048_576,
  "deepseek-v4-flash": 1_048_576,
  // OpenAI (GPT-5 family input ~272K; backend profile caps appropriately)
  "gpt-5.5": 272_000,
  "gpt-5.5-pro": 272_000,
  "gpt-5.4": 272_000,
  "gpt-5.4-mini": 272_000,
  "gpt-5.4-nano": 272_000,
  "gpt-5.1": 272_000,
  "gpt-5": 272_000,
  // Anthropic (standard 200K window; backend caps at 200K)
  "claude-opus-4-8": 200_000,
  "claude-opus-4.8": 200_000,
  "claude-sonnet-4-6": 200_000,
  "claude-haiku-4-5": 200_000,
  "claude-fable-5": 200_000,
  "claude-opus-4-7": 200_000,
  // Google Gemini (1M window)
  "gemini-2.5-pro": 1_048_576,
  "gemini-2.5-flash": 1_048_576,
  "gemini-3-pro-preview": 1_048_576,
  "gemini-3.5-flash": 1_048_576,
  "gemini-3-flash-preview": 1_048_576,
  "gemini-3.1-flash-lite": 1_048_576,
  // Zhipu GLM
  "glm-5": 200_000,
  "glm-5.2": 200_000,
  "glm-4.6": 200_000,
  "glm-4.5": 131_072,
  "glm-4.5-air": 131_072,
  "glm-4.5-flash": 131_072,
  // MiniMax (conservative ~200K to avoid over-budget on the OpenAI-compatible endpoint)
  "minimax-m3": 204_800,
  "minimax-m2": 204_800,
  "minimax-m1": 204_800,
  // xAI Grok
  "grok-4": 256_000,
  "grok-4-fast": 256_000,
  "grok-4-fast-reasoning": 256_000,
  "grok-3": 131_072,
  "grok-3-mini": 131_072,
  "grok-4.3": 1_000_000,
  // Mistral
  "mistral-large-latest": 131_072,
  "mistral-medium-latest": 131_072,
  "mistral-small-latest": 131_072,
  "magistral-medium-latest": 40_960,
  "codestral-latest": 262_144,
  // Moonshot / Kimi
  "kimi-k2-0905-preview": 262_144,
  "kimi-latest": 131_072,
  "moonshot-v1-128k": 131_072,
  "moonshot-v1-32k": 32_768,
  "moonshot-v1-8k": 8_192,
  // Groq-hosted open models
  "llama-3.3-70b-versatile": 131_072,
  "llama-3.1-8b-instant": 131_072,
  // Perplexity Sonar
  "sonar": 128_000,
  "sonar-pro": 200_000,
  "sonar-reasoning": 128_000,
  "sonar-reasoning-pro": 128_000,
  "sonar-deep-research": 128_000,
  // Doubao / Volcengine Ark
  "doubao-seed-1.6": 262_144,
  "doubao-seed-1.6-flash": 262_144,
  "doubao-1.5-pro-256k": 262_144,
  "doubao-1.5-pro-32k": 32_768,
  // Tencent Hunyuan
  "hunyuan-turbos-latest": 131_072,
  "hunyuan-t1-latest": 131_072,
  "hunyuan-large": 131_072,
  // StepFun
  "step-2-16k": 16_384,
  "step-2-mini": 16_384,
  "step-1-32k": 32_768,
  "step-1-8k": 8_192,
};

/** Per-provider fallback window when the exact model id is unknown. */
const PROVIDER_FALLBACK_WINDOW: Partial<Record<ProviderKind, number>> = {
  dashscope: 131_072,
  deepseek: 131_072,
  openai: 272_000,
  openrouter: 131_072,
  anthropic: 200_000,
  gemini: 1_048_576,
  glm: 131_072,
  minimax: 204_800,
  xai: 256_000,
  groq: 131_072,
  mistral: 131_072,
  moonshot: 131_072,
  perplexity: 128_000,
  doubao: 262_144,
  hunyuan: 131_072,
  stepfun: 32_768,
  open_ai_compatible: 131_072,
  local: 32_768,
};

/**
 * Resolve the context window (max input tokens) for a provider+model.
 * Returns null when nothing is known (e.g. custom endpoints / unknown models),
 * in which case the UI should fall back to a manual value.
 */
export function modelContextWindow(provider: ProviderKind, model: string): number | null {
  const key = (model || "").trim().toLowerCase();
  if (key) {
    if (MODEL_CONTEXT_WINDOWS[key]) return MODEL_CONTEXT_WINDOWS[key];
    // OpenRouter uses vendor/model; match on the trailing model id.
    const tail = key.includes("/") ? key.slice(key.lastIndexOf("/") + 1) : key;
    if (tail !== key && MODEL_CONTEXT_WINDOWS[tail]) return MODEL_CONTEXT_WINDOWS[tail];
  }
  return PROVIDER_FALLBACK_WINDOW[provider] ?? null;
}

/**
 * Suggested {maxInput, reservedOutput} budget for a provider+model when the
 * input budget should follow the model context window. Reserves ~12.5% of the
 * window for output, clamped to a sane [1K, 64K] range.
 */
export function autoContextBudget(
  provider: ProviderKind,
  model: string,
): { maxInput: number; reservedOutput: number } | null {
  const window = modelContextWindow(provider, model);
  if (!window) return null;
  const reservedOutput = Math.min(64_000, Math.max(1_024, Math.round(window * 0.125)));
  return { maxInput: window, reservedOutput };
}

export const REASONING_EFFORTS: { value: ReasoningEffort; label: string; hint: string }[] = [
  { value: "auto", label: "Auto", hint: "Provider default" },
  { value: "low", label: "Low", hint: "Fastest" },
  { value: "medium", label: "Medium", hint: "Balanced" },
  { value: "high", label: "High", hint: "Deep" },
  { value: "xhigh", label: "XHigh", hint: "Extended" },
  { value: "max", label: "Max", hint: "Maximum" },
];

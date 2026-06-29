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
    value: "deepseek",
    label: "DeepSeek",
    short: "DS",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
    help: "DeepSeek official OpenAI-compatible endpoint.",
    models: ["deepseek-chat", "deepseek-reasoner"],
  },
  {
    value: "dashscope",
    label: "阿里云百炼 (DashScope)",
    short: "BL",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
    help: "Aliyun Bailian / DashScope OpenAI-compatible endpoint. Media uses native DashScope APIs below.",
    models: ["qwen-max", "qwen-plus", "qwen-turbo", "qwen-long", "deepseek-v3", "deepseek-r1"],
  },
  {
    value: "openai",
    label: "ChatGPT / OpenAI",
    short: "AI",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o",
    help: "OpenAI chat completions endpoint.",
    models: ["gpt-4o", "gpt-4o-mini", "gpt-4.1", "gpt-4.1-mini", "o3", "o4-mini"],
  },
  {
    value: "openrouter",
    label: "OpenRouter",
    short: "OR",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-4o",
    help: "OpenRouter model gateway. Models use the vendor/model form.",
    models: [
      "openai/gpt-4o",
      "anthropic/claude-3.7-sonnet",
      "google/gemini-2.0-flash-001",
      "deepseek/deepseek-chat",
      "meta-llama/llama-3.3-70b-instruct",
    ],
  },
  {
    value: "anthropic",
    label: "Anthropic",
    short: "AN",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-5",
    help: "Claude API. The key is stored in the system credential manager.",
    models: ["claude-sonnet-4-5", "claude-opus-4-1", "claude-3-7-sonnet-latest", "claude-3-5-haiku-latest"],
  },
  {
    value: "gemini",
    label: "Google Gemini",
    short: "GE",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.5-pro",
    help: "Google AI Studio compatible Gemini endpoint.",
    models: ["gemini-2.5-pro", "gemini-2.0-flash", "gemini-2.0-flash-thinking-exp", "gemini-1.5-pro"],
  },
  {
    value: "glm",
    label: "智谱 GLM",
    short: "GL",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-plus",
    help: "Zhipu AI OpenAI-compatible endpoint.",
    models: ["glm-4-plus", "glm-4-air", "glm-4-flash", "glm-4-long"],
  },
  {
    value: "minimax",
    label: "MiniMax",
    short: "MM",
    baseUrl: "https://api.minimax.chat/v1",
    model: "MiniMax-Text-01",
    help: "MiniMax OpenAI-compatible endpoint.",
    models: ["MiniMax-Text-01", "abab6.5s-chat"],
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
    models: ["deepseek-chat", "gpt-4o", "qwen-plus"],
  },
  {
    value: "local",
    label: "Local (Ollama / LM Studio)",
    short: "LO",
    baseUrl: "http://localhost:11434/v1",
    model: "llama3.1",
    help: "Ollama, LM Studio, vLLM or any local OpenAI-compatible service.",
    models: ["llama3.1", "qwen2.5", "deepseek-r1", "phi4", "gemma2"],
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
  "open_ai_compatible",
  "local",
]);

export function findProvider(value: ProviderKind): ProviderOption {
  return PROVIDER_OPTIONS.find((p) => p.value === value) ?? PROVIDER_OPTIONS[0];
}

export const REASONING_EFFORTS: { value: ReasoningEffort; label: string; hint: string }[] = [
  { value: "auto", label: "Auto", hint: "Provider default" },
  { value: "low", label: "Low", hint: "Fastest" },
  { value: "medium", label: "Medium", hint: "Balanced" },
  { value: "high", label: "High", hint: "Deep" },
  { value: "xhigh", label: "XHigh", hint: "Extended" },
  { value: "max", label: "Max", hint: "Maximum" },
];

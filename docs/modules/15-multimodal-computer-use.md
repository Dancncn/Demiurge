# 多模态与 Computer Use 底层能力

> 存档级技术原理文档。覆盖本地 OCR（PP-OCRv5 mobile + oar-ocr 推理）、屏幕感知工具（窗口列表 / 截图 / 区域或窗口 OCR）、语音（云端 ASR + TTS 均已接通）以及云端多模态生成（图像 / TTS）四块底层能力。
>
> 主要源文件：
> - `src-tauri/src/ocr.rs` — OCR 模型管理与推理
> - `src-tauri/src/tools/screen.rs` — 屏幕感知工具
> - `src-tauri/src/voice.rs` — WebView 录音 → 云端 ASR 转写、TTS 合成（dashscope / gpt-sovits 双后端，`voice.rs:193-249`）
> - `src-tauri/src/media.rs` — DashScope 云端图像生成 / 语音合成

---

## 一、模块职责与定位

这一组模块共同构成 Demiurge 的"感官层"：让 Agent 既能**看见屏幕**（截图 + 本地 OCR 把像素变成可推理的文本与坐标），也能**听到语音**（WebView 录音 → 云端转写成文本进 Composer），还能**生成图像 / 语音**（云端多模态）。它们的设计基调是一致的：

1. **本地优先、按需下载、不内置重资产。** OCR 模型不随安装包分发，由用户在设置里按需从 ModelScope 或 Hugging Face 下载到 app data 目录（`ocr.rs:1` 注释明确这一点）。这避免了把上百 MB 的 ONNX 权重塞进安装包。
2. **截图不入上下文。** 屏幕工具把截图落盘到沙盒，只把**文件路径 + 尺寸**返回给模型，而不是把图片二进制灌进对话上下文（`screen.rs:1` 与 `save_capture` 的 `note` 字段）。OCR 工具则进一步把像素就地转成文本再返回，仍然不返回图像本体。
3. **能力门控 + 默认关闭。** 屏幕工具受 `settings.computer_use_enabled` 统一门控（默认 `false`），语音受 `settings.voice_enabled` 门控（默认 `false`）。所有屏幕工具在注册表里都是 `ToolRisk::Privileged` 且 `PermissionPolicy::ask`，必须经过确认门。
4. **如实标注接通状态。** 当前已接通：本地 OCR 推理、屏幕截图/窗口列表/区域与窗口 OCR、云端 ASR 转写（DashScope `qwen3-asr-flash` 或 OpenAI 兼容 Whisper）、云端图像生成、云端 TTS（`media::synthesize_speech`）、voice 模块 TTS（`voice::voice_synthesize`，dashscope + gpt-sovits 双后端，见 §4.3）。**未实现**：TTS 的流式合成、播放队列、打断、语速/情感参数、连接测试与失败降级。

### 当前接通状态一览

| 能力 | 入口 | 状态 | 后端 |
|---|---|---|---|
| OCR 推理 | `ocr::recognize_rgba` | 已接通 | 本地 oar-ocr / PP-OCRv5 mobile ONNX |
| OCR 模型下载 / 状态 | `ocr::download_models` / `model_status` | 已接通 | ModelScope / Hugging Face |
| 屏幕窗口列表 | `screen::list_windows` | 已接通 | xcap |
| 区域 / 窗口截图 | `screen::capture_region` / `capture_window` | 已接通 | xcap |
| 区域 / 窗口 OCR | `screen::ocr_region` / `ocr_window` | 已接通 | xcap + 本地 OCR |
| 语音 ASR（转写） | `voice::voice_transcribe` | 已接通 | DashScope `qwen3-asr-flash` / OpenAI 兼容 `whisper-1` |
| 语音 TTS（voice 模块） | `voice::voice_synthesize` | 已接通 | DashScope `qwen3-tts-flash` / GPT-SoVITS（`http://127.0.0.1:9880`） |
| 云端图像生成 | `media::generate_image` | 已接通 | DashScope `qwen-image-2.0` |
| 云端语音合成 | `media::synthesize_speech` | 已接通 | DashScope `qwen3-tts-flash` |

---

## 二、OCR 子系统（`ocr.rs`）

### 2.1 关键类型与常量

- 三个模型文件常量：`DET_FILE = "pp-ocrv5_mobile_det.onnx"`、`REC_FILE = "pp-ocrv5_mobile_rec.onnx"`、`DICT_FILE = "ppocrv5_dict.txt"`（`ocr.rs:11-13`）。检测模型 + 识别模型 + 字典三件套是 PP-OCRv5 的标准组合。
- `OcrState`（`ocr.rs:15-26`）：唯一的运行时状态，内部是 `Mutex<Option<OAROCR>>`。它作为字段挂在全局 `AppState.ocr` 上（`lib.rs:61`、`lib.rs:99`）。`clear()` 把缓存的引擎置空，用于下载完模型后强制重建。
- `OcrModelSource`（`ocr.rs:28-75`）：枚举 `ModelScope | HuggingFace`，序列化别名为 `modelscope` / `huggingface`。`from_setting` 容错解析（接受 `hf`、`hugging_face` 等别名，**默认回落到 ModelScope**），并附带 `label/note/url` 给前端展示。ModelScope 的 note 写明"适合中国大陆"。
- 状态结构体 `OcrModelStatus` / `OcrModelFileStatus`（`ocr.rs:77-99`）：暴露给前端的安装状态，含 `installed`、每个文件的 `present/bytes/download_url`、`missing` 缺失清单、`total_bytes`、以及 `manual_install_hint`（下载失败时的手动安装指引）。
- 进度事件 `OcrDownloadEvent`（`ocr.rs:101-116`）：通过 Tauri 事件 `"ocr-download-progress"` 推送。
- 输出结构 `OcrLine` / `OcrFrame`（`ocr.rs:118-136`）：`OcrLine` 含文本、置信度 `conf`、包围盒 `(x0,y0,x1,y1)` 以及该帧尺寸 `frame_w/frame_h`；`OcrFrame` 是合并后的多行 + 整段 `text`。

### 2.2 模型目录布局

```
<app_data>/models/ocr/pp-ocrv5-mobile/
    ├─ pp-ocrv5_mobile_det.onnx
    ├─ pp-ocrv5_mobile_rec.onnx
    └─ ppocrv5_dict.txt
```

由 `model_dir(data_dir)` 计算（`ocr.rs:143-145`：`data_dir/models/ocr/pp-ocrv5-mobile`）。`data_dir` 来自 `AppState.data_dir`。

### 2.3 下载流：进度事件 + 临时文件 + 原子落盘

`download_models`（`ocr.rs:156-183`）逐个文件下载，整体串行：

```
download_models(source)
  ├─ create_dir_all(model_dir)
  ├─ source_files(source)  →  3 个 ModelFile{target, url}
  ├─ for (idx, file):  download_one(...)  累加 completed_bytes
  └─ state.ocr.clear()    // 关键：下载完清空旧引擎，下次推理重建
```

`source_files`（`ocr.rs:315-339`）按源拼 URL：
- ModelScope：`https://modelscope.cn/models/greatv/oar-ocr/resolve/master/{target}`，三个文件直接用 `target` 文件名。
- Hugging Face：指向 `monkt/paddleocr-onnx` 仓库里语义不同的子路径（`detection/v5/det.onnx`、`languages/chinese/rec.onnx`、`languages/chinese/dict.txt`），但**落盘时仍重命名为统一的 `DET_FILE/REC_FILE/DICT_FILE`**，所以引擎初始化逻辑对两个源透明。

`download_one`（`ocr.rs:341-419`）的健壮性设计值得记录：

1. 先发 `"starting"` 事件，再 `http.get(url).send()`，并用 `error_for_status()` 把 HTTP 错误状态转成错误。
2. **流式写入**：`response.bytes_stream()` 边收边写到 `{target}.download` 临时文件，每个 chunk 都发一次 `"downloading"` 进度事件，携带"本文件已下载字节"和"全局累计字节 `completed_bytes + downloaded`"。`total_bytes` 来自 `Content-Length`（可能为 `None`）。
3. **空文件保护**：若 `downloaded == 0`，删临时文件并报错（`ocr.rs:400-403`），防止把 0 字节文件当成有效模型。
4. **原子落盘**：`std::fs::rename(tmp, target)` 把临时文件改名为正式文件（`ocr.rs:404`），避免中途崩溃留下半截模型被误判为已安装。
5. 最后发 `"finished"` 事件，`done = true`。

> 注意：`status_for_dir` 判定文件"存在"的标准是 `bytes > 0`（`ocr.rs:281`），不是单纯 `Path::exists`。配合下载阶段的临时文件命名（`.download` 后缀不会被当成正式文件），缺模型检查不会被半截下载欺骗。

### 2.4 缺模型检查与引擎缓存

`ensure_engine`（`ocr.rs:185-209`）是推理前的守门人：

```
ensure_engine(state)
  ├─ status = status_for_dir(model_dir, ModelScope)   // 仅用于查文件齐不齐
  │     └─ if !installed → Err("OCR 模型未安装，缺少：{missing}…请先在设置里下载…")
  ├─ lock engine; if Some → 直接返回（命中缓存）
  └─ OAROCRBuilder::new(det, rec, dict).build()  →  缓存进 OcrState.engine
```

设计要点：

- **缓存键省略源差异。** `ensure_engine` 里 `status_for_dir(&dir, OcrModelSource::ModelScope)` 硬编码了 `ModelScope`，但这里 `source` 只影响 `download_url` 等展示字段，**不影响 `missing` 判定**（判定只看 `model_dir` 下三个固定文件名是否存在），所以即便用户实际用 HF 源下载，缺模型检查依然正确。
- **引擎是重对象，缓存复用。** `OAROCR` 构建一次后缓存在 `Mutex<Option<OAROCR>>`。只有 `clear()`（下载新模型后）会让它失效重建。
- 引擎初始化只传三个路径字符串给 `OAROCRBuilder`（`ocr.rs:200-206`），其余参数走 oar-ocr 默认。

### 2.5 推理与后处理（`recognize_rgba`）

`recognize_rgba(state, rgba)`（`ocr.rs:211-270`）是 OCR 的核心算法入口，被 `screen.rs` 的 `ocr_region/ocr_window` 和 `lib.rs` 的 `ocr_image_bytes` 命令共用。流程：

```
RgbaImage ──to_rgb8()──► engine.predict(vec![rgb]) ──► results[0]
   │
   ├─ if rectified_img.is_some() → Err  // 文档矫正会改坐标，拒绝
   ├─ frame_w/h = input_img 尺寸
   ├─ for region in text_regions:
   │     ├─ (text, conf) = region.text_with_confidence()
   │     ├─ trim；空 or is_noise → skip
   │     └─ 去重：同文本且包围盒 IoU>0.6 视为重复
   ├─ 行排序：按 (y0/8 取整, x0) 升序   // 8px 行容差，近似"先上后左"
   ├─ text = lines.join("\n")
   └─ if text 为空 → Err("未识别到文本")
```

几个细节及其原因：

- **拒绝文档矫正（`ocr.rs:222-225`）**：oar-ocr 若触发文档矫正（`rectified_img`），识别到的坐标是矫正后图像上的坐标，无法安全映射回原始屏幕像素，因此直接报错并提示改用普通屏幕区域。这是为了保证返回坐标对"截图区域"语义上可信——下游可能拿坐标去点击。
- **噪声过滤 `is_noise`（`ocr.rs:454-456`）**：字母数字字符少于 2 个的串视为噪声，过滤掉分隔符、单字符等干扰。
- **去重 `boxes_overlap`（`ocr.rs:458-469`）**：标准 IoU 计算，阈值 0.6。仅当文本相同且框高度重叠时才判重，避免重复行。
- **行排序的 8px 容差（`ocr.rs:255-260`）**：把 `y0` 按 8 像素分桶，使同一视觉行（即便 y 略有抖动）排在一起，再按 x 排序，得到自然阅读顺序。

### 2.6 OCR 的对外命令面

`lib.rs` 注册了三个 Tauri 命令：`ocr_image_bytes`（`lib.rs:1410`，对任意图片字节做 OCR，只返回纯文本）、`ocr_model_status`（`lib.rs:1603`）、`ocr_download_models`（`lib.rs:1608`，异步，带 `AppHandle` 用于发进度事件）。

---

## 三、屏幕感知工具（`tools/screen.rs`）

### 3.1 工具清单与注册

五个工具，全部是 **deferred tool**（不进主 schema，靠 `tool_search` 发现后由 `execute_tool` 代理执行），见 `tools/mod.rs:145-152` 的 `DEFERRED_TOOL_NAMES` 与 `execute_tool.rs:20-27` 的分发表：

| 工具名 | 入口函数 | 输出策略 |
|---|---|---|
| `screen_list_windows` | `list_windows` | TruncateForUi |
| `screen_capture_region` | `capture_region` | Inline |
| `screen_capture_window` | `capture_window` | Inline |
| `screen_ocr_region` | `ocr_region` | TruncateForUi |
| `screen_ocr_window` | `ocr_window` | TruncateForUi |

五者在注册表里都是 `ToolRisk::Privileged` + `ToolConcurrency::SerialOnly` + `PermissionPolicy::ask(...)`（`tools/mod.rs:630-714`），权限提示里明确"可能包含密钥、聊天或其它隐私信息"。`SerialOnly` 是因为截图涉及全局屏幕状态，串行更可控。

> 设计动机：把这些低频、隐私敏感的工具放进 deferred pool，可以减少固定 tools JSON 对模型上下文的占用（`IMPLEMENTATION.md:275`），只有真正需要时才用 `tool_search` 拉进来。

### 3.2 统一门控

每个入口第一行都是 `ensure_enabled(state)`（`screen.rs:176-183`），读取 `settings.computer_use_enabled`，关闭时返回"Computer Use 未启用"。注意 `preview_*` 系列函数（`screen.rs:138-174`）**不调用** `ensure_enabled`，因为它们只生成确认门用的人类可读预览文案、不真正读屏。

### 3.3 截图实现（xcap + 显示器边界校验）

底层全部走 `xcap`（跨平台截屏库）。`capture_screen_region(x, y, w, h)`（`screen.rs:255-300`）是核心：

```
1. 用区域中心点 (x+w/2, y+h/2) 选显示器：xcap::Monitor::from_point(cx, cy)
      失败则回落到 Monitor::all() 里的 primary 显示器
2. 读显示器原点 (mx,my) 与尺寸 (mw,mh)
3. 把全局坐标换算成显示器本地坐标 local = (x-mx, y-my)
4. 边界校验：local<0 或 local+尺寸 超出 (mw,mh) → 报错"跨越显示器边界或超出屏幕"
5. monitor.capture_region(local_x, local_y, w, h)
```

**为什么用中心点选屏**：多显示器下，区域应归属于它中心所在的那块屏；这样比用左上角更鲁棒。**为什么拒绝跨屏**：`xcap` 的 `capture_region` 是对单个 monitor 的局部截图，跨屏区域无法在一次调用里完成，且坐标语义会混乱，所以宁可报错也不裁切。

尺寸校验 `validate_capture_size`（`screen.rs:344-358`）在所有截图入口前调用：单边不超过 `MAX_CAPTURE_SIDE = 8192`，总像素不超过 `MAX_CAPTURE_PIXELS = 33_000_000`（约 33MP），零尺寸直接拒。这是防止超大截图把内存/磁盘打爆。

### 3.4 窗口枚举与匹配

`read_window`（`screen.rs:185-207`）把 `xcap::Window` 转 `WindowInfo`，过滤掉：最小化窗口、标题与应用名都为空的窗口、以及宽 < 80 或高 < 60 的迷你窗口（噪声）。

`find_window(title, app)`（`screen.rs:209-225`）：遍历所有窗口，`title` 全等或 `app` 全等即命中，多个命中时**取面积最大的**（`area > b.4`）。这处理了同一应用多窗口的情况——通常面积最大的是主窗口。注意匹配是**精确相等**而非模糊包含，所以前端/模型需要先用 `screen_list_windows` 拿到准确标题。

### 3.5 窗口裁剪几何

`capture_window` 与 `window_capture_rect`（`screen.rs:64-101`、`227-253`）支持 `crop_left/top/right/bottom` 四个 0~1 比例参数：

```
x = wx + ww*l ;  y = wy + wh*t
width = ww*(r-l) ; height = wh*(b-t)
其中 r = clamp(crop_right).max(l+0.02)，b 同理（保证最小 2% 宽高，不退化成 0）
```

比例裁剪而非绝对像素，意味着模型只需说"截窗口右半边"就能用 `crop_left=0.5`，不必先知道窗口精确尺寸。

### 3.6 落盘与返回（截图不入上下文）

`save_capture`（`screen.rs:302-342`）把 RGBA 图保存为 PNG：

- 路径：`<sandbox>/.demiurge/screenshots/<sanitized_label>_<毫秒时间戳>.png`。`sanitize_label`（`screen.rs:386-403`）只保留 `[A-Za-z0-9_-]`、截断到 48 字符，全被替换则用 `"capture"`。
- 返回 JSON 含绝对 `path`、相对 `relative_path`（统一用 `/` 分隔，见 `relative_path_to_string`）、区域坐标尺寸，以及关键的 `note`：**"当前工具不返回图像内容；后续 OCR/视觉模型可读取该文件。"** 这是"截图不入上下文"原则的落地点。

`ocr_region/ocr_window`（`screen.rs:103-136`）则在截图后立刻调 `crate::ocr::recognize_rgba`，返回 `{ ok, region, text, lines }`——**注意它们不落盘 PNG**，直接把识别结果返回，因为 OCR 的目的就是把像素转成文本/坐标。

---

## 四、语音 ASR（`voice.rs`）

### 4.1 数据流：WebView 录音 → 云端转写

语音转写的录音发生在前端（WebView 的 `MediaRecorder`，默认产出 `audio/webm`），后端只负责把音频字节转发给云端 STT 端点。`voice_transcribe`（`voice.rs:70-121`）是 Tauri 命令：

```
voice_transcribe(audio: Vec<u8>, mime_type?, language?)
  ├─ if !voice_enabled → Err
  ├─ if audio.is_empty() → Err
  ├─ mime 默认 "audio/webm"；language 可选
  └─ match settings.voice_stt_backend (小写):
        ├─ "dashscope" → key=dashscope_api_key；
        │     url = {dashscope_base}/compatible-mode/v1/audio/transcriptions
        │     model = "qwen3-asr-flash"
        ├─ "openai"    → key=settings.api_key（当前供应商）；
        │     url = {settings.base_url}/audio/transcriptions
        │     model = "whisper-1"
        ├─ "none"/""   → Err（未选后端）
        └─ other       → Err（未知后端）
```

两个后端都汇聚到 `transcribe_multipart`（`voice.rs:125-172`），它构造 OpenAI Whisper 风格的 multipart：

- 按 `mime` 推断 `file_name`（`.m4a/.wav/.mp3/.webm`），因为有的服务端靠扩展名判格式。
- 表单字段：`file`（音频 part，带 `mime_str`）、`model`，可选 `language`。
- `bearer_auth(api_key)` 鉴权，POST 后取响应 JSON 的 `text` 字段并 trim；空文本或缺字段报错。

> 这里两个后端**走同一种请求形状**（OpenAI `/audio/transcriptions` multipart），DashScope 通过其"compatible-mode"兼容端点接入。这是刻意的：用一个 `transcribe_multipart` 同时覆盖两个后端，减少分叉。

### 4.2 凭据解析与就绪判定

`dashscope_api_key`（在 `media.rs:70-85`，被 voice 复用）的解析优先级：
1. `settings.media_api_key`（媒体面板专用 key）；
2. 若当前 `provider == DashScope`，回落到 LLM 的 `settings.api_key`；
3. 再回落到环境变量 `DASHSCOPE_API_KEY`。

`dashscope_base_url`（`media.rs:57-68`）会把用户填的 base 末尾的 `/compatible-mode/v1` 剥掉，得到裸 base，再由各处按需拼路径——避免用户填了兼容端点 base 时产生 `/compatible-mode/v1/compatible-mode/v1` 这种重复。

`voice_status` / `stt_ready`（`voice.rs:27-66`）给前端返回"是否真正可用"：要求 `voice_enabled` + 选了支持的后端 + 对应凭据可解析，并返回中文 `reason` 说明为什么不可用。

### 4.3 TTS 合成（`voice_synthesize`）

`voice_synthesize`（`voice.rs:193-249`）是 voice 模块的 TTS 入口，按 `settings.voice_tts_backend` 分发到双后端：

```
voice_synthesize(text, voice_id?, state)
  ├─ if !voice_enabled → Err
  ├─ text = trim(text); 空 → Err
  └─ match voice_tts_backend (小写):
        ├─ "dashscope"/"aliyun"/"bailian"/"media"
        │     voice 回落链: requested_voice_id → settings.voice_id
        │                    → settings.tts_voice → "Cherry"
        │     → media::synthesize_speech(SpeechSynthesisRequest{
        │           text, model=settings.tts_model, voice, language_type="Chinese"
        │        }) → 返回 result.url（DashScope 音频 URL）
        ├─ "gpt-sovits"/"gpt_sovits"/"gptsovits"
        │     → synthesize_with_gpt_sovits(http, settings, text, requested_voice_id)
        │        默认 base http://127.0.0.1:9880 → 返回 base64 data URI
        ├─ "none"/"" → Err（未选后端）
        └─ other → Err（未知后端，仅支持 dashscope / gpt-sovits）
```

dashscope 分支复用 §5 的 `media::synthesize_speech`，模型默认 `qwen3-tts-flash`、音色默认 `Cherry`；gpt-sovits 分支走本地/外部 HTTP 服务，返回 base64 data URI，可直接喂给前端 `<audio>`。流式合成、播放队列、打断、语速/情感参数、连接测试与失败降级尚未实现（见 `docs/TODO.md`）。

> 两个 TTS 入口的分工：`voice::voice_synthesize` 是 voice 模块面向 Composer/角色包 voice 偏好的统一入口，dashscope 分支复用 `media::synthesize_speech`；`media::synthesize_speech`（§5）则是 media 面板独立调用 DashScope 云端 TTS 的薄封装。两者 dashscope 路径同源。

相关默认值（`store/mod.rs`）：`voice_enabled` 默认 `false`，`voice_stt_backend`/`voice_tts_backend` 默认 `"none"`，`voice_id` 默认空串。

---

## 五、云端多模态生成（`media.rs`）

`media.rs` 是 DashScope 多模态生成的薄封装，提供两个能力，均通过统一的 `dashscope_post`（`media.rs:136-158`）打到 `{base}/api/v1/services/aigc/multimodal-generation/generation`（`AIGC_GENERATION_PATH`）。

### 5.1 图像生成 `generate_image`（`media.rs:160-217`）

- 模型回落链：请求体 `model` → `settings.image_model` → `"qwen-image-2.0"`；尺寸同理回落到 `settings.image_size` → 由 `normalize_dashscope_size` 把 `1024x1024` 形式的 `x/X` 统一替换成 DashScope 要求的 `*`（`media.rs:87-94`）。
- 参数：`prompt_extend`、`watermark`、可选 `seed`；负向提示拼成 `"Negative prompt: ..."` 追加进 content。
- 响应解析 `collect_image_urls`（`media.rs:104-134`）做了**多形态兼容**：依次尝试 `output.choices[].message.content[].image`、`output.results[].url|image_url`、`output.image_url`，以容忍 DashScope 不同模型返回结构的差异。无 URL 则报错。

### 5.2 语音合成 `synthesize_speech`（`media.rs:219-263`）

- 模型/音色回落：`model` → `settings.tts_model` → `"qwen3-tts-flash"`；`voice` → `settings.tts_voice` → `"Cherry"`；`language_type` 默认 `"Chinese"`。
- 请求体走 `input.{text,voice,language_type}`，响应取 `output.audio.url`。
- 对外命令：`media_generate_image` / `media_synthesize_speech`（`lib.rs:1418-1430`）。

媒体相关默认值（`store/mod.rs:283-336`）：`media_base_url` 默认 `https://dashscope.aliyuncs.com`，`image_model` 默认 `qwen-image-2.0`，`tts_model` 默认 `qwen3-tts-flash`，`tts_voice` 默认 `Cherry`，`media_api_key` 默认空。

---

## 六、与其他模块的交互边界

```
                ┌─────────────── 前端 (React/WebView) ───────────────┐
                │  MediaRecorder 录音        Settings 面板/下载进度   │
                └──────┬──────────────────────────────┬─────────────┘
                       │ invoke                        │ listen("ocr-download-progress")
                       ▼                               ▲
   ┌───────────────────────────────────────────────────────────────┐
   │                        Tauri 命令层 (lib.rs)                    │
   │  voice_transcribe / voice_synthesize / voice_status            │
   │  media_generate_image / media_synthesize_speech                │
   │  ocr_image_bytes / ocr_model_status / ocr_download_models      │
   └───┬───────────────┬───────────────┬───────────────────┬───────┘
       ▼               ▼               ▼                   ▼
   voice.rs   ──key──► media.rs     ocr.rs            tools/screen.rs
   (ASR/TTS wired)    (DashScope)   (OAROCR缓存)      (xcap截图/窗口)
                                        ▲                   │
                                        └──recognize_rgba───┘   (ocr_region/ocr_window)

   Agent 工具调用链：tool_search → execute_tool(execute_tool.rs) → screen::*
```

- **screen ↔ ocr**：`screen::ocr_region/ocr_window` 截图后直接调 `ocr::recognize_rgba`，是 OCR 推理的主要调用方之一。
- **voice ↔ media**：voice 模块复用 media 模块的 `dashscope_api_key` / `dashscope_base_url`（`voice.rs:12`），凭据与 base URL 解析逻辑统一在 media 一处。
- **全局状态 `AppState`**：`OcrState` 是 `AppState.ocr` 字段（`lib.rs:61`）；截图落盘依赖 `AppState.sandbox_dir`；模型目录依赖 `AppState.data_dir`；HTTP 请求统一走 `AppState.http`（共享 `reqwest::Client`）。
- **工具注册 / 权限门**：屏幕工具的风险等级、确认提示、deferred 归类都在 `tools/mod.rs` 定义，执行经 `tools/execute_tool.rs` 代理（`is_deferred_tool` 校验）。

---

## 七、安全与权限相关点

1. **截图不入上下文**：`save_capture` 只返回路径，不返回图像字节，避免隐私图像直接进对话历史（`screen.rs:339`）。
2. **双重门控 + 确认门**：`computer_use_enabled` / `voice_enabled` 默认关闭；屏幕工具全是 `Privileged` + `ask`，每次截图/OCR 都要用户确认，确认提示明示可能含密钥/聊天等隐私（`tools/mod.rs:635-714`）。
3. **沙盒约束**：截图只写入 `<sandbox>/.demiurge/screenshots/`，文件名经 `sanitize_label` 清洗，杜绝路径穿越或非法字符。
4. **资源上限**：`MAX_CAPTURE_SIDE`/`MAX_CAPTURE_PIXELS` + 跨屏拒绝，防止异常大或越界截图。
5. **凭据来源分层**：媒体/ASR 的 key 优先用专用 `media_api_key`，避免和 LLM key 强绑定；支持环境变量 `DASHSCOPE_API_KEY` 兜底。`store` 在导出/备份时会清空 `media_api_key`（`store/mod.rs:434`），不外泄密钥。
6. **OCR 坐标可信性**：拒绝文档矫正结果，保证返回坐标对原始屏幕像素可映射，避免下游基于错误坐标做点击类操作。

---

## 八、已知限制与扩展点

- **voice TTS 仍缺流式/队列/打断**：`voice::voice_synthesize` 已接通 dashscope + gpt-sovits 双后端（`voice.rs:193-249`）；尚未实现的是流式合成、播放队列、打断、语速/情感参数、连接测试与失败降级（`docs/TODO.md`）。
- **ASR 无流式/无热键**：当前是"录完整段再上传转写"的一次性请求，尚无流式转写或热键触发（`docs/TODO.md:87`）。
- **窗口匹配为精确相等**：`find_window` 不支持模糊/包含匹配；窗口标题动态变化（如带页码、未读数）时需先 `list_windows` 取准确标题。
- **截图不跨屏**：单次截图只能落在一块显示器内。
- **OCR 单语言/单模型**：当前仅 PP-OCRv5 mobile 中文三件套；HF 源也是 chinese rec/dict。多语言需扩展 `source_files` 与字典。
- **OCR 无 GPU 配置面**：`OAROCRBuilder` 仅传三路径，未暴露线程数/后端选择等调优参数。
- **`download_one` 进度风暴**：每个 chunk 发一次事件，超大文件可能产生高频事件；前端需自行节流。

---

## 九、现有文档与代码不符之处（顺手发现）

1. **`docs/IMPLEMENTATION.md:101`** 旧描述把 `voice.rs` 写成"TTS/ASR command surface 预留，设置可见但后端未接入"。实际 ASR 与 TTS 均已接通：`voice_transcribe` 走 DashScope `qwen3-asr-flash` 或 OpenAI 兼容 Whisper；`voice_synthesize`（`voice.rs:193-249`）走 dashscope + gpt-sovits 双后端。该行已订正为"STT + TTS 已接通"。`docs/TODO.md` 已相应把 TTS 标 `[x]`、剩余流式/队列/打断拆为 `[ ]`。
2. **`docs/IMPLEMENTATION.md:163` 的目录布局**写作 `ocr-models/`（沙盒 `.demiurge/` 树下），与实际 OCR 模型路径 `<app_data>/models/ocr/pp-ocrv5-mobile/`（`ocr.rs:143-145`）不一致：OCR 模型存在 **app data** 而非沙盒 `.demiurge/`，且目录名是 `models/ocr/...` 而非 `ocr-models/`。`IMPLEMENTATION.md:86` 另处又写 `ocr-models.md`（文档名），与模型目录名易混淆。

# 19 — Live2D 面板（MVP）

本篇讲 Demiurge 如何把一个 Cubism 4/5 Live2D 模型挂到角色包上、在应用内渲染出来，以及当前 MVP 的边界与待打磨项。面向想扩展桌宠外壳或接 TTS 口型同步的协作者。

> 面向用户的功能介绍见 [README.md](../../README.md)；路线图与下一步见 [TODO.md](../TODO.md) 的 P4 段；角色包清单字段细节见 [14-pack-system](14-pack-system.md)。

## 1. 方案定位

Live2D 是角色包的**可选素材**，与 `avatar`（静态头像）并列。当前落地的是「应用内面板 MVP」：在主窗口里开一个 Live2D 视图，挂载模型、跑 idle 物理/眨眼/呼吸，支持缩放和拖拽。**透明置顶桌宠窗口、TTS 口型同步、动作播放**都是后续阶段，本文末尾「待打磨」一节列出。

## 2. 关键选型与为什么

### 2.1 渲染库：`untitled-pixi-live2d-engine`

早期 TODO 写的是 `pixi-live2d-display`（guansss 原版）。调研后改用 `untitled-pixi-live2d-engine`，原因：

- 原版 `pixi-live2d-display` 最后更新停在 2023-12，`peerDependencies` 锁 `@pixi/* ^6`，**不支持 Pixi v7/v8**。作者在 issue #166 说会做 v8 新版但从未发布。
- `pixi-live2d-display-lipsyncpatch`（RaSan147 fork）活跃，但锁死 Pixi v7。
- `untitled-pixi-live2d-engine` 是原库作者在 issue #181 公开宣告的 v8 + Cubism 5 继任者：原生 Pixi v8 渲染管线、Cubism 2–5、MIT、2026 年仍活跃维护。

本项目用 Vite 6 + React 18，配 Pixi v8 最顺，故选本库。安装：`pixi.js@^8`、`@pixi/sound@^6`（引擎 peer，SoundManager 在模块加载期就引用）、`untitled-pixi-live2d-engine`。

### 2.2 资源加载：Tauri asset 协议（不是 data URL，也不是 `read_pack_file`）

Live2D 模型是多 MB 的 `.moc3` + 多张纹理 + `.physics3.json` + `.cdi3.json`，且 `.model3.json` 以**相对路径**引用这些 sibling 文件。这与 `avatar`（单张图，base64 成 `avatarDataUrl` data URL 塞进 manifest）完全不同——把几 MB 的 moc3 + 纹理 base64 进清单既爆 `MAX_PACK_READ_BYTES`，也破坏相对引用。

引擎的 `CubismModelSettings` 拿到 model3.json 的 URL 后，用 `new URL(relative, base)` 原生解析 sibling，再逐个 fetch。所以只要给引擎一个**能原生解析相对路径的 base URL**，它自己会把 `.moc3`/纹理/物理/cdi 全部取回来。

Tauri 2 的 asset 协议正好满足：`convertFileSrc(绝对路径)` 把本地路径转成 `https://asset.localhost/<编码路径>`（Windows），引擎从这个 URL fetch model3.json，再相对解析出 sibling 的 asset URL，全部命中 asset scope。

实现：

- `tauri.conf.json` 的 `app.security.assetProtocol` 开 `enable: true`，scope `["$APPDATA/packs/**"]`（`$APPDATA` 解析到 app data dir，即 `packs_dir`）。
- `Cargo.toml` 给 `tauri` 加 `protocol-asset` feature（开启 assetProtocol 时 Tauri 2 强制要求，否则 build script 报错）。
- 前端不直接读文件字节，而是调 `resolve_pack_live2d_path` 拿绝对路径，再 `convertFileSrc` 转 URL 交给引擎。

> 现有的 `read_pack_file`（`pack/mod.rs`）对 `.moc3` 这类非文本非图片二进制返回空（`PackFileContent { text: None, data_url: None }`），对 Live2D 不可用——这反证了 asset 协议才是正道。

### 2.3 Cubism Core：私有运行时，用户自取，动态注入

`live2dcubismcore.min.js`（moc3 解析运行时，WASM 内嵌其中，无独立 .wasm）受 Live2D Proprietary Software License 约束，**禁止第三方再分发**。所以它不入库，由用户自行下载：

- `scripts/fetch-cubism-core.mjs` 从 `cubism.live2d.com` 官方地址下载到 `public/core/live2dcubismcore.min.js`（失败则打印手动下载指引）。
- `.gitignore` 排除该文件，`public/core/.gitkeep` 占位保目录。
- 运行时由 `src/lib/live2d.ts` 的 `ensureCubismCore()` **动态**创建 `<script>` 标签注入 `<head>`，缺失时抛出指向 `npm run fetch:cubism-core` 的友好错误。不写进 `index.html`，保持懒加载——只有用户打开 Live2D 面板才加载。

非商业用途免费；商业用途需遵守 Live2D SDK Release License。

### 2.4 bundle 隔离

Pixi v8 + 引擎 + `@pixi/sound` 体积大（构建后 `vendor-live2d` chunk 约 1.1MB / 308KB gzip）。为不污染主 bundle：

- `src/lib/live2d.ts` 里所有 `pixi.js` / `untitled-pixi-live2d-engine/cubism` 的 import 都是 `await import(...)` 动态形式。
- `src/components/Live2DPanel.tsx` 用 `export default`，`src/App.tsx` 用 `React.lazy(() => import(...))` + `<Suspense>` 挂载。
- `vite.config.ts` 的 `manualChunks` 把 `pixi.js` / `@pixi` / `untitled-pixi-live2d-engine` 归到 `vendor-live2d` chunk。

效果：用户不点 Live2D nav，这些代码不会下载/执行。

## 3. 数据流

```text
设置 > 人物包 > Live2D 模型 > 选择文件夹
  └─ @tauri-apps/plugin-dialog open({directory:true})
       └─ invoke import_pack_live2d_folder(packId, srcDir)
            └─ pack::import_live2d_folder
                 ├─ 校验源目录有且仅有 1 个 .model3.json
                 ├─ 清空并重建 <pack>/live2d/，递归复制（文件数/字节数上限）
                 ├─ read_manifest_no_avatar → manifest.live2d = "live2d/<model>.model3.json"
                 ├─ validate_manifest_paths + validate_pack_files
                 └─ 写回 manifest.json，返回更新后的 PackManifest
       └─ 前端刷新 packs + manifest JSON 编辑器

侧栏 > Live2D
  └─ Live2DPanel 挂载（React.lazy + Suspense）
       └─ loadModel()
            ├─ invoke resolve_pack_live2d_path(packId) → 绝对路径
            ├─ convertFileSrc(absPath) → https://asset.localhost/.../xxx.model3.json
            └─ loadLive2DModel(url, canvas)
                 ├─ ensureCubismCore() → 动态注入 live2dcubismcore.min.js
                 ├─ 动态 import pixi.js + untitled-pixi-live2d-engine/cubism
                 ├─ extensions.add(Live2DPlugin)（仅首次，模块级守卫）
                 ├─ await app.init({ preference:"webgl", backgroundAlpha:0, resizeTo })
                 ├─ configureCubismSDK({ memorySizeMB:32 })
                 └─ Live2DModel.from(url, { textureOptions:{lod:"single-auto"}, autoUpdate:true })
                      └─ 引擎以 url 为 base，fetch .moc3 / 纹理 / .physics3.json / .cdi3.json（全部走 asset 协议）
```

## 4. 关键文件

| 关注点 | 位置 |
|---|---|
| manifest 字段 | `src-tauri/src/pack/mod.rs:44` `pub live2d: Option<String>` |
| 路径校验 | `src-tauri/src/pack/mod.rs` `validate_manifest_paths`（live2d 块：相对路径 + `.model3.json` 后缀） |
| 存在性校验 | `src-tauri/src/pack/mod.rs` `validate_pack_files`（live2d 块） |
| 文件夹导入 | `src-tauri/src/pack/mod.rs:814` `import_live2d_folder` + `:873` `copy_live2d_dir_recursive` |
| 路径解析 | `src-tauri/src/pack/mod.rs:912` `resolve_live2d_model_path`（返回绝对路径） |
| 移除 | `src-tauri/src/pack/mod.rs:930` `remove_live2d` |
| Tauri 命令 | `src-tauri/src/lib.rs:945/956/963` 三个 `#[tauri::command]`，`:2333` 起注册，`:2256` dialog 插件 |
| asset 协议 | `src-tauri/tauri.conf.json` `app.security.assetProtocol` |
| dialog 权限 | `src-tauri/capabilities/default.json` `dialog:default` |
| Cargo feature | `src-tauri/Cargo.toml` `tauri = { features = ["protocol-asset"] }` + `tauri-plugin-dialog` |
| 引擎初始化 | `src/lib/live2d.ts:21` `ensureCubismCore`、`:56` `loadLive2DModel` |
| 面板组件 | `src/components/Live2DPanel.tsx:14`（canvas 生命周期、缩放、拖拽、重载） |
| 设置 UI | `src/components/SettingsDialog.tsx:2272` Live2D Section、`:1001/1023` 导入/移除 handler、`:839` `currentPackManifest` |
| Cubism Core 下载 | `scripts/fetch-cubism-core.mjs` |
| bundle 隔离 | `vite.config.ts` `manualChunks`（`vendor-live2d`）+ `src/App.tsx` `React.lazy` |

## 5. 引擎 API 注意点

- `extensions.add(Live2DPlugin)` 必须在 `app.init()` 之前注册，否则 live2d 渲染管线不会安装。`live2d.ts` 用模块级 `engineInitialized` 守卫，只在首次 `loadLive2DModel` 调用时注册。
- `preference: "webgl"` 必须显式传——Live2D 渲染管线是 WebGL-only，Pixi v8 默认 `auto-detect` 可能选 WebGPU 导致模型不渲染。
- `Application` 在 Pixi v8 是异步的：`await app.init(...)` 之后才能 `addChild`。
- `configureCubismSDK({ memorySizeMB: 32 })` 把 Cubism Core 工作内存从默认 16MB 提到 32MB，避免复杂/4096 纹理模型卡更新。
- `textureOptions: { lod: "single-auto" }` 让引擎按屏幕尺寸按需生成降采样图集，规避 4096 纹理在低显存 WebView 上超 `MAX_TEXTURE_SIZE`。
- **`eyeBlink` / `breathDepth` 不是 `Live2DFactoryOptions` 的有效字段**（那是原 `pixi-live2d-display` 的 API）。Untitled 引擎默认就开自动眨眼（`EyeBlink` 组存在时驱动 `ParamEyeLOpen`/`ParamEyeROpen`）和 CubismBreath（呼吸/微晃），无需显式传。要关掉得在加载后改 `model.internalModel`，MVP 未暴露这个开关。

## 6. 限制与待打磨

### 6.1 当前 MVP 不做
- **透明置顶桌宠窗口**：独立 OS 窗口、`transparent` + `alwaysOnTop` + `set_ignore_cursor_events` 点击穿透、可收起/展开。技术上 Tauri 2 在 Win11 完全可行（无需额外 Cargo feature），但需要第二个 Vite 入口 + 独立 capability + 透明窗口配置，工作量另开一轮。见 TODO P4「Live2D 桌宠方案」。
- **口型同步**：`model3.json` 的 `LipSync` 组为空（`Ids: []`），且本项目 TTS adapter 尚未接通。等 TTS 落地后，要么给 `LipSync` 组补 `ParamMouthOpen` 让 `model.speak(audioUrl)` 自动驱动，要么每帧 `model.internalModel.coreModel.setParameterValueById("ParamMouthOpen", v)` 手动驱动。
- **动作播放**：`model3.json` 无 `Motions` 字段，引擎不合成 idle 动作。当前靠 CubismBreath + 自动眨眼 + `physics3.json` 让模型「活着」，但没有全身 idle 动画。要真动作需作者 `.motion3.json` 并在 model3.json 加 `Idle` 组。
- **眨眼/呼吸开关**：面板只有缩放和重载，没暴露眨眼/呼吸 toggle（引擎 API 不支持 factory option 级开关，要在 `internalModel` 上改，留到下一轮）。

### 6.2 已知风险点
- **CJK 路径**：模型路径含 `三月七` 等非 ASCII 字符。`convertFileSrc` 会 URL 编码，asset 协议解码后应能命中。若引擎 fetch sibling 报 404，先查 asset scope 是否真覆盖到含 CJK 的绝对路径，必要时把 scope 放宽到 `$APPDATA/**` 排查。
- **4096 纹理**：`texture_00.png` / `texture_01.png` 是 4096×4096。`lod: "single-auto"` 会按需降采样；若仍 OOM 或纹理黑，检查 GPU VRAM（4096² RGBA ≈ 64MB/张）。
- **Cubism Core 缺失**：用户未跑 `npm run fetch:cubism-core` 时，`ensureCubismCore` 的 `onerror` 会抛「Failed to load Cubism Core. Run: npm run fetch:cubism-core」，面板进 error 态。
- **`$APPDATA` scope 解析**：Tauri 2 的 `$APPDATA` 应解析到含 identifier 的 app data dir（`.../com.demiurge.engine/packs`）。若 asset 报 "not allowed"，先确认解析结果是否含 identifier，必要时放宽 scope。
- **License**：Cubism Core 受 Live2D Proprietary Software License 约束（非商业免费，商业需 Release License）。本项目不分发该文件，由用户自行下载接受许可。

### 6.3 测试覆盖
- `pack/mod.rs` 的 `validates_manifest_identity_and_paths` 已补 `live2d: None` + `credits` + `license` 字段（此前字面量缺 `credits`/`license` 会导致 `cargo test` 编译失败，顺手修）。
- **未覆盖**：`import_live2d_folder` / `resolve_live2d_model_path` / `remove_live2d` 的行为测试尚未补（涉及真实文件系统复制，可仿 `imports_zip_pack_and_exposes_avatar_data_url` 的临时目录模式补）。

## 7. 扩展指引

- **接 TTS 口型同步**：TTS adapter 落地后，在 `Live2DPanel` 里订阅 TTS 音频事件，用 `model.internalModel.coreModel.setParameterValueById("ParamMouthOpen", rms)` 每帧驱动；或给 model3.json 的 `LipSync` 组补 `ParamMouthOpen` 后调 `model.speak(audioUrl)`。
- **桌宠窗口**：在 `tauri.conf.json` 加第二个 window（`label: "pet"`，`transparent/decorations:false/alwaysOnTop:true/skipTaskbar:true`），新建 `src/pet.tsx` 只挂 Pixi+Live2D，加 `capabilities/pet.json`，用 `app.emit_to("pet", ...)` 从 agent 循环驱动表情。主窗口保持普通装饰窗口不变。
- **状态映射**：Companion 的 `focus`/`mood` 状态变化时，通过 `model.internalModel` 调参数或播动作，低频触发，避免干扰工作。

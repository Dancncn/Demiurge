# 19 — Live2D 面板与独立窗口

> 审阅状态（2026-07-26）：模型改为通过 Tauri asset protocol 直接加载，不再把整套模型转成 base64 经 IPC 传输；应用内面板保持挂载，独立透明置顶窗口按需首次加载并在隐藏时暂停 ticker。生产构建中的 Live2D vendor 约 1.1 MB，仍触发非阻断体积警告；口型/动作联动仍未实现。

本篇讲 Demiurge 如何把一个 Cubism 4/5 Live2D 模型挂到角色包上，在应用内面板和独立透明窗口中复用渲染资源，以及当前边界与待打磨项。

> 面向用户的功能介绍见 [README.md](../../README.md)；路线图与下一步见 [TODO.md](../TODO.md) 的 P4 段；角色包清单字段细节见 [14-pack-system](14-pack-system.md)。

## 1. 方案定位

Live2D 是角色包的**可选素材**，与 `avatar`（静态头像）并列。当前已落地应用内常驻面板和独立透明置顶窗口：模型运行 idle 物理/眨眼/呼吸，支持缩放、拖拽、鼠标跟随开关、重载和窗口隐藏/复用。**点击穿透、TTS 口型同步、表情/动作状态映射**仍是后续阶段。

## 2. 关键选型与为什么

### 2.1 渲染库：`untitled-pixi-live2d-engine`

早期 TODO 写的是 `pixi-live2d-display`（guansss 原版）。调研后改用 `untitled-pixi-live2d-engine`，原因：

- 原版 `pixi-live2d-display` 最后更新停在 2023-12，`peerDependencies` 锁 `@pixi/* ^6`，**不支持 Pixi v7/v8**。作者在 issue #166 说会做 v8 新版但从未发布。
- `pixi-live2d-display-lipsyncpatch`（RaSan147 fork）活跃，但锁死 Pixi v7。
- `untitled-pixi-live2d-engine` 是原库作者在 issue #181 公开宣告的 v8 + Cubism 5 继任者：原生 Pixi v8 渲染管线、Cubism 2–5、MIT、2026 年仍活跃维护。

本项目用 Vite 6 + React 18，配 Pixi v8 最顺，故选本库。安装：`pixi.js@^8`、`@pixi/sound@^6`（引擎 peer，SoundManager 在模块加载期就引用）、`untitled-pixi-live2d-engine`。

### 2.2 资源加载：受检路径 + Tauri asset protocol

Live2D 模型是 `.model3.json` + `.moc3` + 纹理 + 物理/Pose/DisplayInfo/UserData/表情/动作/声音的引用图。清单只保存 model3 相对路径，不把大模型内联进 manifest；打开面板时直接从本地 asset URL 加载：

- `resolve_pack_live2d_path` 先在后端 canonicalize 包根和 model 文件，复用导入阶段的 containment 边界，只返回当前角色包内已验证模型的绝对路径。
- `createLive2DAssetModelUrl` 用 `convertFileSrc` 生成 model3 的 asset URL，读取 JSON 后把 Moc、纹理、Physics、Pose、DisplayInfo、UserData、表情、动作与声音引用全部改写为完整 asset URL。
- 改写完整 URL 是 Windows WebView 的必要条件：若保留 model3 内相对引用，纹理会相对 asset host 的错误目录解析并返回 403。
- `.model3.json`、`.moc3`、纹理和 physics 由 WebView 直接读取，不再把整个资源图编码为 JSON/base64 走 IPC；重写后的 model3 仅生成一个小型 blob URL，并在失败、重载或卸载时 revoke。

asset protocol scope 只放行应用数据目录的 `packs/**`；导入时仍会对全部 model3 引用做便携路径、普通文件和 canonical containment 校验。旧 `pack_live2d_bundle` 命令保留供兼容与测试使用，但不再位于渲染主路径。

### 2.3 Cubism Core：私有运行时，用户自取，动态注入

`live2dcubismcore.min.js`（moc3 解析运行时，WASM 内嵌其中，无独立 .wasm）受 Live2D Proprietary Software License 约束，**禁止第三方再分发**。所以它不入库，由用户自行下载：

- `scripts/fetch-cubism-core.mjs` 从 `cubism.live2d.com` 官方地址下载到 `frontend/public/core/live2dcubismcore.min.js`（失败则打印手动下载指引）。
- `.gitignore` 排除该文件，`frontend/public/core/.gitkeep` 占位保目录。
- 运行时由 `frontend/src/lib/live2d.ts` 的 `ensureCubismCore()` **动态**创建 `<script>` 标签注入 `<head>`，缺失时抛出指向 `npm run fetch:cubism-core` 的友好错误。不写进 `index.html`，保持懒加载——只有用户打开 Live2D 面板才加载。

非商业用途免费；商业用途需遵守 Live2D SDK Release License。

### 2.4 bundle 隔离

Pixi v8 + 引擎 + `@pixi/sound` 体积大（构建后 `vendor-live2d` chunk 约 1.1MB / 308KB gzip）。为不污染主 bundle：

- `frontend/src/lib/live2d.ts` 里所有 `pixi.js` / `untitled-pixi-live2d-engine/cubism` 的 import 都是 `await import(...)` 动态形式。
- `frontend/src/features/live2d/Live2DPanel.tsx` 用 `export default`，`frontend/src/app/App.tsx` 用 `React.lazy(() => import(...))` + `<Suspense>` 挂载。
- `vite.config.ts` 的 `manualChunks` 把 `pixi.js` / `@pixi` / `untitled-pixi-live2d-engine` 归到 `vendor-live2d` chunk。

效果：用户不点 Live2D nav，这些代码不会下载/执行。

## 3. 数据流

```text
设置 > 人物包 > Live2D 模型 > 选择文件夹
  └─ @tauri-apps/plugin-dialog open({directory:true})
       └─ invoke import_pack_live2d_folder(packId, srcDir)
            └─ pack::import_live2d_folder
                 ├─ 获取 Live2D mutation lock；canonicalize 源/目标
                 ├─ 在包内建立唯一 staging；只复制普通文件（200 文件 / 200 MB 上限）
                 ├─ staging 顶层必须恰有一个 .model3.json
                 ├─ 全 FileReferences 做便携路径 + canonical containment 校验
                 ├─ 非 ASCII 引用只在 staging 内移动到 normalized_assets/ 并全部重写
                 ├─ 完整复核候选模型树，预写并 sync manifest 临时文件
                 ├─ 备份旧 live2d/ 与 manifest，rename 提交新目录/清单
                 └─ 最终复核失败则回滚旧目录和清单；成功才清理备份
       └─ 前端刷新 packs + manifest JSON 编辑器

侧栏 > Live2D
  └─ Live2DPanel 首次访问后保持挂载（React.lazy + Suspense）
       └─ loadModel()
            ├─ invoke resolve_pack_live2d_path(packId) → 受检 model3 绝对路径
            ├─ createLive2DAssetModelUrl() → 完整 asset URL 引用 + 小型 model blob URL
            └─ loadLive2DModel(modelUrl, canvas)
                 ├─ ensureCubismCore() → 动态注入 live2dcubismcore.min.js
                 ├─ 动态 import pixi.js + untitled-pixi-live2d-engine/cubism
                 ├─ extensions.add(Live2DPlugin)（仅首次，模块级守卫）
                 ├─ await app.init({ preference:"webgl", backgroundAlpha:0, resizeTo })
                 ├─ configureCubismSDK({ memorySizeMB:128 })
                 └─ Live2DModel.from(url, { textureOptions:{lod:false}, autoUpdate:true })
                       └─ 引擎通过完整 asset URL 读取 moc、纹理、physics 等本地资源

独立 Live2D 窗口
  ├─ Tauri 启动时隐藏预创建透明 webview，首次呼出才挂载模型
  ├─ 关闭请求改为 hide，保留 WebGL、模型和纹理实例
  └─ live2d-visibility-changed → 隐藏 ticker.stop() / 显示 ticker.start()
```

## 4. 关键文件

| 关注点 | 位置 |
|---|---|
| manifest 字段 | `backend/Demiurge-desktop/src/pack/manifest.rs` `PackManifest.live2d` |
| manifest 路径/存在性 | `validate_manifest_paths` / `validate_pack_files` |
| 事务导入与回滚 | `backend/Demiurge-desktop/src/pack/live2d.rs` `import_live2d_folder` / `install_prepared_live2d` |
| 内部引用边界 | `normalize_live2d_reference` / `resolve_model_relative_file` / `collect_live2d_refs_checked` |
| 受检资源读取 | `live2d_bundle` / `resolve_live2d_model_path` |
| 移除 | `backend/Demiurge-desktop/src/pack/live2d.rs` `remove_live2d` |
| Tauri 命令 | `import_pack_live2d_folder` / `resolve_pack_live2d_path` / `pack_live2d_bundle` / `remove_pack_live2d` |
| dialog 权限 | `backend/Demiurge-desktop/capabilities/default.json` `dialog:default` |
| asset URL 改写 | `frontend/src/lib/live2d.ts` `createLive2DAssetModelUrl` / `rewriteModelReferences` |
| 引擎初始化 | `frontend/src/lib/live2d.ts` `ensureCubismCore` / `loadLive2DModel` |
| 面板组件 | `frontend/src/features/live2d/Live2DPanel.tsx`（canvas/ticker 生命周期、进度、鼠标跟随、缩放、拖拽、重载） |
| 独立窗口壳 | `frontend/src/features/live2d/Live2DWindowShell.tsx` + `backend/Demiurge-desktop/src/biz/window.rs`（隐藏预创建、显隐事件、实例复用） |
| 设置 UI | `frontend/src/features/settings/SettingsDialog.tsx` Live2D 导入/移除区域 |
| Cubism Core 下载 | `scripts/fetch-cubism-core.mjs` |
| bundle 隔离 | `vite.config.ts` `manualChunks`（`vendor-live2d`）+ `frontend/src/app/App.tsx` `React.lazy` |

## 5. 引擎 API 注意点

- `extensions.add(Live2DPlugin)` 必须在 `app.init()` 之前注册，否则 live2d 渲染管线不会安装。`live2d.ts` 用模块级 `engineInitialized` 守卫，只在首次 `loadLive2DModel` 调用时注册。
- `preference: "webgl"` 必须显式传——Live2D 渲染管线是 WebGL-only，Pixi v8 默认 `auto-detect` 可能选 WebGPU 导致模型不渲染。
- `Application` 在 Pixi v8 是异步的：`await app.init(...)` 之后才能 `addChild`。
- `configureCubismSDK({ memorySizeMB: 128 })` 为复杂模型预留更充足的 Cubism Core 工作内存。
- 当前 `textureOptions: { lod: false }`，加载后还会检查每张 texture 是否有 source；这能把纹理失败显式转成面板错误，但不会主动降低 4096 纹理显存占用。
- **`eyeBlink` / `breathDepth` 不是 `Live2DFactoryOptions` 的有效字段**（那是原 `pixi-live2d-display` 的 API）。Untitled 引擎默认就开自动眨眼（`EyeBlink` 组存在时驱动 `ParamEyeLOpen`/`ParamEyeROpen`）和 CubismBreath（呼吸/微晃），无需显式传。要关掉得在加载后改 `model.internalModel`，MVP 未暴露这个开关。

## 6. 限制与待打磨

### 6.1 当前版本不做
- **完整桌宠交互**：独立透明置顶窗口已经实现，但仍保留工具栏和鼠标交互，没有点击穿透、自动收起、Companion 状态到表情/动作的映射。
- **口型同步**：`model3.json` 的 `LipSync` 组为空（`Ids: []`），且本项目 TTS adapter 已接通但 Live2D lip-sync 尚未接入。lip-sync 待接入时，要么给 `LipSync` 组补 `ParamMouthOpen` 让 `model.speak(audioUrl)` 自动驱动，要么每帧 `model.internalModel.coreModel.setParameterValueById("ParamMouthOpen", v)` 手动驱动。
- **动作播放**：`model3.json` 无 `Motions` 字段，引擎不合成 idle 动作。当前靠 CubismBreath + 自动眨眼 + `physics3.json` 让模型「活着」，但没有全身 idle 动画。要真动作需作者 `.motion3.json` 并在 model3.json 加 `Idle` 组。
- **眨眼/呼吸开关**：当前已提供鼠标跟随开关，但眨眼/呼吸仍未暴露 toggle（引擎 API 不支持 factory option 级开关，要在 `internalModel` 上改）。

### 6.2 已知风险点
- **大纹理与 GPU 体积**：资源已不再经 base64 IPC，但当前关闭 LOD。4096² RGBA 纹理展开约 64 MB/张，超大模型仍可能放大 WebView 内存和 GPU 压力；独立窗口隐藏时会停 ticker，但保留纹理以换取再次呼出的低延迟。
- **进程异常退出的隐藏备份**：普通函数错误会执行目录/manifest 回滚；若进程在极短的 rename 提交窗口被强制终止，包目录可能留下 `.live2d-*.bak/.tmp`，后续可增加启动恢复/清理日志进一步加固崩溃一致性。
- **Cubism Core 缺失**：用户未跑 `npm run fetch:cubism-core` 时，`ensureCubismCore` 的 `onerror` 会抛「Failed to load Cubism Core. Run: npm run fetch:cubism-core」，面板进 error 态。
- **License**：Cubism Core 受 Live2D Proprietary Software License 约束（非商业免费，商业需 Release License）。本项目不分发该文件，由用户自行下载接受许可。

### 6.3 测试覆盖

`pack::live2d::tests` 使用真实临时目录覆盖：

- Unix 根路径、UNC、Windows drive/prefix、`.`/`..`、空组件、尾点/空格与保留设备名拒绝；
- 绝对、父目录和 Windows 路径导入失败时，旧 manifest/模型及包外文件字节保持不变；
- Moc、纹理、Physics、Expression、Motion、Sound 的非 ASCII 文件全部在 staging 内改名并重写，bundle 可完整读取；
- 源 symlink 与已有模型引用 symlink 逃逸均拒绝；
- 故意让 manifest 提交失败，验证新目录被移除、旧目录和旧 manifest 恢复且无临时 artifact。
- 前端回归覆盖 asset URL/完整引用改写、阶段进度、主面板 keep-alive、独立窗口透明配置与隐藏/显示 ticker 生命周期；下拉菜单定位另有纯几何测试。

`remove_live2d` 的显式删除成功/失败分支仍可补独立行为测试；它不影响本次不可信导入的安全闭环。

## 7. 扩展指引

- **接 TTS 口型同步**：TTS adapter 已就绪（dashscope + gpt-sovits 双后端），lip-sync 待接入。在 `Live2DPanel` 里订阅 TTS 音频事件，用 `model.internalModel.coreModel.setParameterValueById("ParamMouthOpen", rms)` 每帧驱动；或给 model3.json 的 `LipSync` 组补 `ParamMouthOpen` 后调 `model.speak(audioUrl)`。
- **扩展桌宠窗口**：现有 `live2d` 窗口已承担透明置顶与资源复用；后续可增加点击穿透/交互模式切换，并用定向事件从 agent 循环驱动表情和动作。
- **状态映射**：Companion 的 `focus`/`mood` 状态变化时，通过 `model.internalModel` 调参数或播动作，低频触发，避免干扰工作。

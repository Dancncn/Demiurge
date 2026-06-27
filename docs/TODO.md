# TODO / 路线图

Demiurge 的待办与方向，欢迎认领。✅ 已完成 · ⬜ 待做 · 💡 想法。
设计背景见 [demiurge-mvp-design.md](./demiurge-mvp-design.md)，实现见 [IMPLEMENTATION.md](./IMPLEMENTATION.md)。

## MVP 现状 ✅
- [x] OpenAI 兼容流式 LLM 适配器（默认 DeepSeek）
- [x] Agent 循环（调用→工具→喂回→重复）+ 流式 + 中断
- [x] 工具：`open_path` / `read_file` / `write_file` / `web_search` / `system_info`
- [x] 权限门（auto / confirm + 前端确认弹窗）
- [x] 文件沙盒（词法 + canonicalize 双重校验，防链接逃逸）
- [x] 上下文裁剪、会话/设置持久化（重启恢复）
- [x] **多会话**：侧栏会话列表（新建 / 切换 / 删除），首条消息自动生成标题，多份对话持久化
- [x] 角色包加载（清单 + 人格），首启动落地默认包
- [x] ChatGPT 风浅色 UI（侧栏 + 居中栏 + 悬浮输入框 + Markdown/代码/数学）+ 莲花品牌 + 浅紫渐变

## 近期打磨 ⬜
- [ ] **API Key 安全存储**：从明文 `settings.json` 改为系统凭据管理器（Windows keyring）
- [ ] **会话重命名**：手动改会话标题
- [ ] **角色包头像**：读取 `manifest.avatar`，替换 UI 里的默认莲花头像
- [ ] **设置里一键测试连接**：填完 Key 后点一下验证 base_url/model 可用
- [ ] **错误展示**：把 LLM/网络错误更友好地呈现在气泡里（含重试）
- [ ] **更多内置工具**（示例）：`list_dir`、`http_get`、`clipboard` 等（注意权限分级）
- [ ] **i18n**：界面文案中英切换
- [ ] **打包产物瘦身**：按需拆分 misans/katex，减小首屏体积

## 已设计、待实现（来自设计文档）🔜
- [ ] **TTS / ASR 适配器**：语音输出（先接外部 GPT-SoVITS / CosyVoice HTTP）/ 语音输入
  - 角色包新增字段：`tts.backend` / `tts.voice`
  - 复用现有「流式逐句」管线做逐句合成
- [ ] **桌宠视觉外壳**：透明置顶窗口、Live2D 模型、点击穿透、主动说话、屏幕感知
  - 角色包新增字段：`live2d.model` / `expressions` / 情绪映射 / 问候·待机台词
- [ ] **角色包导入器**：UI 内导入/管理角色包（拖入 zip 解压到 `packs/`）

## 明确不做（避免范围蔓延）🚫
- 向量 / 长期记忆 RAG —— 持久化已覆盖 MVP 记忆
- 多 Agent / 工作流编排 —— 单循环对一个伴侣足够

## 想法 💡
- 工具调用的「计划预览」——执行前让模型先列出将做的事
- 角色包市场 / 分发格式规范化（仍不含 IP）
- 跨平台验证（目前主要在 Windows 11 开发；macOS/Linux 路径与 `open_path` 已写分支但未充分测试）

## 贡献提示
- 加工具/角色包/换端点的具体步骤见 IMPLEMENTATION.md「如何扩展」。
- 提交前请跑 `npm run build`（含 `tsc`）与 `cargo build`（在 `src-tauri/`）确保两端均通过。
- 不要把具体角色的素材（美术 / 语音 / 基于特定作品的人格）提交进仓库，角色包属于用户本地。

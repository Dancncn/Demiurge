# GOAI 赛事适配说明

当前分支已经补充一个可运行的脱敏模拟包：使用 72 个合成 A2L 测量信号、6 个模拟 ECU、合成 CAN/MDA 回放和虚构试验文档，能够生成原始 XLSX、正式 DOCX、校验结果和运行证据。该包是初赛可选代码包和复赛前的可复现基础，不代表真实企业或硬件链路已完成验收。

> 适用分支：`goai`
>
> 本分支以官方 AgentTeams 作为多 Agent 协同与运行治理平面。Demiurge 保留本地 Agent loop、桌面工具、权限确认和文件工作区能力，作为本地工具能力与离线 fallback；不把自研 Workflow 冒充为 AgentTeams。

本次初赛适配先交付理论可行的场景、资源、Skill 和工具契约；真实 MCP Server、AgentTeams 部署、企业凭据接入和 ECU/MDA 实测属于后续阶段，不在本文中宣称已经完成。

## 1. 适配原则

AgentTeams 负责：

- Manager 对任务进行路由和跨团队协调；
- Team Leader 在团队房间内拆解任务并分派 Worker；
- Worker 按职责调用 Skill、MCP 和共享状态；
- Matrix 房间提供可见、可干预、可审计的协作记录；
- Higress 负责 LLM / MCP 统一入口、Consumer Token 和真实凭据隔离；
- MinIO 保存任务规格、Worker 结果、报告和知识沉淀。

Demiurge 负责：

- 本地项目工作区、文件预览、桌面窗口和人工确认；
- 本地 Skill 编辑与调试；
- A2L、测量结果和报告契约的可视化；
- 需要本地桌面或企业内网时的 MCP 工具宿主。

AgentTeams 官方设计强调 Manager、Team、Worker 和 Human 的声明式资源，以及 Manager 不直接穿透 Team、由 Team Leader 负责团队内分工。对应资源见 [agentteams-goai.yaml](../competition/agentteams/agentteams-goai.yaml)。

## 2. ECU 环境测试示例闭环

该场景只是可复用能力的示例，不绑定具体企业、车型或工具品牌：

```text
用户提交 A2L 文件、MDA 测量入口、ECU 清单、试验大纲、流程手册和报告规范
        │
        ▼
AgentTeams Manager
        │  路由到 Trial Report Team
        ▼
Trial Report Team Leader（仅编排，不承担业务产物）
        ├── 数据处理专家 Agent
        │     解析 A2L、编排 MDA 测量，生成版本化 Excel 原始数据
        ├── 合规流程 Agent
        │     解析试验大纲和流程手册，生成步骤、验收和证据要求
        └── 报告 Agent
              根据原始 Excel、合规模型和报告规范生成正式试验报告
        │
        ▼
人工确认高风险桌面操作
        │
        ▼
校验通过 → 共享任务状态、报告、校验摘要和审计证据
验证失败 → 标记失败原因、保留原始结果、重试幂等步骤或请求人工介入
```

当前演示具体采用车载冷藏设备控制器这一脱敏对象：合成 A2L 包含 72 个测量信号，任务配置包含 6 个模拟 ECU；MDA 具体界面、CAN 卡、TSMaster 数据路径和企业规则仍属于可替换输入适配层，不写死在 AgentTeams 编排层，因此可以迁移到其他 ECU、台架、日志分析或工程测试场景。

公开资料只用于支撑场景合理性：ASAM MCD-2 MC 说明 A2L/AML 的 ECU 测量描述用途，ASAM MDF 说明测量数据归档与总线数据存储，ISO 16750-4、GB/T 28046.4、IEC 60068-2-14 和 IEC 60068-2-30 用于环境测试类别的公开语境；具体试验参数仍以授权试验大纲为准。资料索引见 [GOAI 公开资料审查](GOAI-PUBLIC-REFERENCE-AUDIT.md)。

## 3. Agent Identity 映射

| Identity | AgentTeams 资源 | 职责边界 | 主要输入 | 主要输出 |
|---|---|---|---|---|
| `goai-manager` | Manager | 接收人类任务、选择团队、汇总结果；不直接执行团队内工具 | 任务描述、ECU/项目上下文 | 团队任务、最终摘要、待确认事项 |
| `trial-report-lead` | Team Leader Worker | 拆解任务、分派三类业务 Worker、协调状态；不伪造测量或合规结论 | 任务规格、验收标准 | 子任务、依赖关系、汇总包 |
| `data-processing-expert` | Worker | 数据处理专家 Agent：解析 A2L、调用批准的 MDA 流程并生成 Excel 原始数据；不生成正式结论 | A2L、ECU 清单、信号范围、测量参数 | 信号目录、原始测量行、版本化 `raw-data.xlsx` |
| `compliance-process-agent` | Worker | 合规流程 Agent：解析试验大纲和流程手册，提取步骤、验收、报告章节和证据义务 | 试验大纲、流程手册、版本号 | `compliance-model.json`、报告规范和缺口清单 |
| `report-agent` | Worker | 报告 Agent：按报告规范组合原始 Excel 与合规模型，生成并校验正式试验报告 | 原始 Excel、合规模型、报告规范、证据 | `trial-report.docx/pdf`、章节校验和证据索引 |
| `human-operator` | Human | 审批桌面操作、改变测量范围、接受异常结果 | 确认请求、证据摘要 | allow / deny / retry / rollback 决策 |

其中三个业务 Worker 是参赛方案要求的三个不同职能 Agent；`trial-report-lead` 是 AgentTeams 为团队协同增加的编排角色，不作为第四个业务职能，也不生成测量数据、合规模型或正式报告。完整身份字段、能力边界、工具权限、失败策略和交接格式记录在各 Skill 文件与 AgentTeams Worker 的 `soul` 中。

## 4. 赛事技术要求对应关系

| 赛事要求 | `goai` 分支落点 |
|---|---|
| AgentTeams 协同基点 | `competition/agentteams/agentteams-goai.yaml` |
| 至少 3 个不同职能 Agent | 数据处理专家、合规流程、报告 3 个业务 Worker；Team Leader 仅编排，不计入业务职能 |
| Agent Identity 清单 | 本文第 3 节 |
| Skill 必选 | `packs/default/skills/goai-*` |
| MCP 工具契约 | `competition/agentteams/mcp-contract.yaml` |
| 共享状态 / RAG | MinIO `shared/tasks`、`shared/knowledge`；Demiurge 本地 memory / Lorebook 作为本地 fallback |
| 结果验证 | `goai-excel-report-validator`、`trial_validate_report`、报告契约和验收规则 |
| 执行证据 | Matrix 房间历史、MinIO `meta.json/spec.md/result.md`、MCP 审计事件、Demiurge 本地 JSONL 审计 |
| 审批与回滚 | Human 资源 + Higress/Worker 权限边界；失败步骤只允许幂等重试，报告采用新版本写入，不覆盖原始数据 |
| 可观测 | 首阶段使用 Matrix/MinIO/本地审计；后续接入 AgentLoop 或 AgentScope Studio 输出 Trace、Metrics 和评测结果 |

## 5. 交付边界

当前可交付的本地包见 competition/agentteams/demo/，操作手册见 competition/agentteams/demo/OPERATOR-GUIDE.md，压缩包见 competition/goai-agentteams-package.zip。离线模式不需要 Key；可选 --llm 模式读取当前终端的 DEEPSEEK_API_KEY，只复核合成中间结果。

本次安全配置下已完成一次真实 DeepSeek API 验证：`deepseek-v4-flash` 的最小连接请求成功，应用侧记录约 130 ms；随后用一次性本地桥接把安全凭据仅传给演示子进程，完整三 Agent 模拟也成功生成 `llm-review.json`。演示调用显式关闭 V4 Flash thinking 输出，只保留最终复核文本，不保存或展示内部推理内容。当前模拟结论仍为 `needs_human_review`，因为样例故意包含 1 个被拒绝的 A2L 文件。

初赛只需要把场景、方案 PPT、Identity、Skill、MCP 契约和 AgentTeams 映射讲清楚；复赛再补充 AgentTeams 可执行部署、真实 MCP Server、样例数据、运行日志、验证结果和 Demo。

不为了“堆工具”额外引入 Nacos、PolarDB、RocketMQ 等组件。只有当它们承担明确的注册、向量/审计存储或事件可靠投递职责时才接入；替代方案必须保留接口契约和迁移路径。

## 6. 推荐组件落地与替代关系

| 推荐组件 | `goai` 分支选择 | 迁移边界 |
|---|---|---|
| AgentTeams | 方案上采用，作为唯一参赛多 Agent 协同基点；初赛不宣称已完成线上部署 | `agentteams-goai.yaml` 的 Manager/Worker/Team/Human 资源契约 |
| Higress | 作为后续真实 MCP/LLM 接入与凭据隔离方案；当前仅保留接口和占位地址 | MCP URL、Consumer Token、按 Worker 的 allowed consumer 策略 |
| MinIO | 作为后续共享文件和任务状态方案；当前仅定义共享状态契约 | `shared/tasks/task-{id}` 与 `shared/knowledge` 文件契约 |
| 云 Skills / Nacos | 首阶段使用仓库内可审计 Skill；需要团队级分发时迁移到 Nacos/AgentTeams Skill Registry | Skill 名称、frontmatter 和输入输出契约保持不变 |
| PolarDB for PostgreSQL | 首阶段不引入；任务规模较小时 MinIO 文件状态更容易复核 | 向量、长记忆和审计记录使用稳定 ID，可将存储适配到 PostgreSQL/pgvector |
| UnifiedModel | 首阶段不引入；当前使用显式 JSON 数据契约 | `task_id`、`signal_id`、`capture_id`、`report_checksum` 作为跨系统关联键 |
| RocketMQ | 首阶段使用 AgentTeams Matrix 房间和 MinIO 状态流转；不伪造可靠消息队列语义 | 未来可把 `task.created`、`capture.completed`、`report.validated` 映射为幂等事件 |
| AgentLoop / AgentScope Studio | 首阶段保留 Matrix 历史、本地审计和证据包；复赛接入完整 Trace/Metrics/评测 | 每次调用保留 Agent、Skill、MCP、task_id、耗时、状态和 evidence URI |

这套取舍符合赛事“推荐项目和云产品不按使用数量评分”的原则：先把必要的协同、工具契约、权限边界和闭环证据做实，再按部署规模替换存储、事件和观测后端。

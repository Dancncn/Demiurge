# Skill 清单

字段与参赛手册附录 B 一致。完整实现说明位于 `packs/default/skills/goai-*/SKILL.md`。

## 1. GOAI Task Decomposer

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI Task Decomposer |
| Skill 类型 | 自定义 Skill / AgentTeams 编排能力 |
| 使用场景 | 在任何外部测量或报告写入前生成有边界的任务图 |
| 输入参数 | 任务描述、文档 URI、ECU/信号范围、时长、报告规范、验收规则 |
| 输出结果 | `task_spec`、阶段、Agent Identity、验收条件、证据位置、升级规则 |
| 调用条件 | Team Leader 首次拆解任务或输入修订版发生变化 |
| 依赖工具 / 系统 | AgentTeams 共享状态；可选 A2L 目录发现工具 |
| 失败处理 | 返回缺失输入清单并阻断下游，不推断 ECU 或阈值 |
| 权限与安全 | 只读任务元数据；不能批准桌面动作或覆盖旧产物 |
| 复用价值 | 可用于测试、运维、质检等多阶段工程任务拆解 |

## 2. GOAI Data Processing Expert

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI Data Processing Expert |
| Skill 类型 | 自定义业务 Skill / 多工具组合 |
| 使用场景 | 将 A2L 与批准的测量数据转换为可追溯 Excel 原始数据 |
| 输入参数 | 任务 ID、A2L URI、ECU/信号范围、采集参数、模板 |
| 输出结果 | 目录 checksum、capture ID、原始行、XLSX URI/checksum、诊断与证据 |
| 调用条件 | 任务范围已固定、报告生成尚未开始 |
| 依赖工具 / 系统 | A2L、MDA、Excel 三类 Skill 与 MCP 工具 |
| 失败处理 | 仅对传输错误幂等重试；保留部分采集和旧版本，歧义转人工 |
| 权限与安全 | 只访问批准 URI/ECU/信号；真实桌面或台架动作前需人工确认 |
| 复用价值 | 可替换测量工具、ECU 数量、信号范围和采样窗口 |

## 3. GOAI A2L Signal Catalog

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI A2L Signal Catalog |
| Skill 类型 | 自定义 Skill / 外部工具封装 |
| 使用场景 | A2L 批量可用性预筛和确定性信号目录生成 |
| 输入参数 | `source_uri`、编码、ECU/信号过滤条件 |
| 输出结果 | source checksum、`signals[]`、`messages[]`、解析诊断、目录 checksum |
| 调用条件 | 每个新 source checksum 调用一次 |
| 依赖工具 / 系统 | `ecu_read_a2l` MCP、共享任务目录 |
| 失败处理 | 格式错误返回源位置；仅重试传输故障 |
| 权限与安全 | 只读批准 URI；不跟随任意网络位置、不宣称已测量 |
| 复用价值 | 与 MDA、ECU 数量和车型解耦，可服务标定与信号目录场景 |

## 4. GOAI MDA Measurement Capture

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI MDA Measurement Capture |
| Skill 类型 | 外部工具封装 / 企业测量系统集成 |
| 使用场景 | 按已批准范围采集或回放测量值 |
| 输入参数 | 任务 ID、目录 checksum、信号 ID、ECU ID、时长、幂等键 |
| 输出结果 | capture ID、原始行、状态、时间戳、source reference、evidence URI |
| 调用条件 | 数据处理专家已固定目录与测量范围，且人工确认完成 |
| 依赖工具 / 系统 | `mda_capture_measurements` MCP、共享目录 |
| 失败处理 | 同幂等键重试瞬态错误；部分结果保留并转人工 |
| 权限与安全 | 不把凭据写入提示词/日志；不得自行扩大信号范围 |
| 复用价值 | 同一合同可适配真实 MDA、MDF/MF4、CAN 回放和其他台架 |

## 5. GOAI Excel Raw-Data Validator

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI Excel Raw-Data Validator |
| Skill 类型 | 自定义 Skill / Excel 工具封装 |
| 使用场景 | 生成版本化 Excel 原始数据并独立重算统计值 |
| 输入参数 | 原始测量行、目录 checksum、capture ID、模板、验收规则 |
| 输出结果 | XLSX、工作簿 checksum、逐 ECU/信号统计、缺失清单、验证证据 |
| 调用条件 | capture ID 稳定后写入；交给报告 Agent 前验证 |
| 依赖工具 / 系统 | Excel build/validate MCP 工具 |
| 失败处理 | 统计不一致时保留原始采集和旧报告，修正后写新版本 |
| 权限与安全 | 只能写批准输出目录，不修改 A2L、原始采集或其他工作区 |
| 复用价值 | 可用于任何结构化工程测量表和报告前置校验 |

## 6. GOAI Compliance Process Agent

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI Compliance Process Agent |
| Skill 类型 | 自定义业务 Skill / 文档结构化 |
| 使用场景 | 将试验大纲、流程手册和报告规范转为合规模型 |
| 输入参数 | 任务 ID、批准文档、修订标识、项目规则 |
| 输出结果 | 规则、步骤、前置条件、证据义务、偏差和报告章节要求 |
| 调用条件 | 任务范围确定后、报告 Agent 判定前；文档修订时重跑 |
| 依赖工具 / 系统 | 文档解析 MCP、共享任务目录 |
| 失败处理 | 缺失、歧义或版本冲突时转人工并阻断报告生成 |
| 权限与安全 | 只读批准文档；不能修改要求或替代人工批准偏差 |
| 复用价值 | 与具体 ECU 和报告模板解耦，可用于质检、合规和实验室流程 |

## 7. GOAI Report Agent

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI Report Agent |
| Skill 类型 | 自定义业务 Skill / 报告工具封装 |
| 使用场景 | 用原始数据、合规模型和模板生成并验证正式报告 |
| 输入参数 | 任务 ID、XLSX URI/checksum、capture 证据、合规模型、报告规范 |
| 输出结果 | 版本化报告、checksum、章节证据、偏差、缺失证据和 verdict |
| 调用条件 | 三类输入存在且 checksum/修订版与任务一致 |
| 依赖工具 / 系统 | report build/validate MCP、共享证据状态 |
| 失败处理 | 数据与要求冲突时保留输入并返回 `fail`/`needs_human_review` |
| 权限与安全 | 只写批准目录；不得发明结果、改规则或覆盖已接受版本 |
| 复用价值 | 更换大纲、流程和模板即可支持其他工程试验报告 |

## 8. GOAI Evidence Auditor

| 字段 | 内容 |
| --- | --- |
| Skill 名称 | GOAI Evidence Auditor |
| Skill 类型 | 自定义治理 Skill / 可观测与审计 |
| 使用场景 | 完成前核验每个主张是否有持久化证据 |
| 输入参数 | Agent handoff、任务 ID、工具结果、共享产物 |
| 输出结果 | verdict、置信度、发现、不确定项、下一步和 evidence URI |
| 调用条件 | Team Leader 宣布完成前，或 Worker 返回 partial/failed/review 时 |
| 依赖工具 / 系统 | AgentTeams 历史、任务产物、MCP 审计记录、Demiurge 本地日志 |
| 失败处理 | 缺证据的主张标为 unverified，不因 Agent 自报成功而降级 |
| 权限与安全 | 只读，不修改测量、报告、权限或任务规范 |
| 复用价值 | 可迁移到运维、支持、研究和其他需要证据链的多 Agent 场景 |

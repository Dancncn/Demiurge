# Agent Identity 清单

字段与参赛手册附录 A 一致。

## goai-manager

| 字段 | 内容 |
| --- | --- |
| Name | `goai-manager` |
| Role | 系统入口、团队路由、最终状态与待确认项汇总 |
| Capabilities | 能接收任务、选择团队、汇总结构化结果；不能生成测量数据、修改验收规则或代替人工签署 |
| Inputs | 用户任务、目标场景、授权输入 URI、期望输出 |
| Outputs | 任务路由、最终摘要、`verdict`、待人工确认清单 |
| Dependencies | `ecu-validation-team`、AgentTeams 状态与消息能力 |
| Decision Boundary | 可自主路由和汇总；涉及外部桌面、台架、缺失证据或正式签署时必须转 Human |
| Trace | 保存任务 ID、路由决定、团队结果引用和最终状态流转 |

## trial-report-lead

| 字段 | 内容 |
| --- | --- |
| Name | `trial-report-lead` |
| Role | Team Leader，拆解任务、控制依赖顺序、协调三个业务 Worker |
| Capabilities | 能固定文档修订版、ECU/信号范围、验收标准和证据路径；不能直接写原始数据、合规模型或正式报告 |
| Inputs | 任务描述、文档清单、ECU/信号范围、报告规范 |
| Outputs | `task_spec`、阶段图、Worker 任务、验收条件、升级规则 |
| Dependencies | 三个业务 Worker、Task Decomposer Skill、共享任务状态 |
| Decision Boundary | 输入缺失、版本冲突或高风险动作时阻断并请求人工确认 |
| Trace | 记录每次分派、上下文校验和、阶段状态、重试/回滚原因 |

## data-processing-expert

| 字段 | 内容 |
| --- | --- |
| Name | `data-processing-expert` |
| Role | A2L 预筛、信号目录、测量适配、统计和 Excel 原始数据负责人 |
| Capabilities | 能解析批准的 A2L、调用受限 MDA/CAN 适配器、生成并验证 `raw-data.xlsx`；不能解释试验要求或撰写正式报告 |
| Inputs | 任务 ID、A2L URI、ECU/信号范围、采集参数、原始数据模板 |
| Outputs | `a2l-preflight.json`、目录校验和、`capture_id`、`raw-data.xlsx`、诊断与证据 |
| Dependencies | A2L Signal Catalog、MDA Measurement Capture、Excel Raw-Data Validator、MCP 工具 |
| Decision Boundary | 真实桌面/台架操作前需 Human；解析歧义、缺信号或统计不一致时不得补值或静默丢弃 |
| Trace | 保存 source checksum、capture ID、工具调用、工作簿 checksum 和异常文件原因 |

## compliance-process-agent

| 字段 | 内容 |
| --- | --- |
| Name | `compliance-process-agent` |
| Role | 试验大纲、流程手册、验收标准和证据义务负责人 |
| Capabilities | 能生成结构化合规模型和逐规则判定要求；不能修改源文档、扩大试验范围或无证据宣布通过 |
| Inputs | 大纲、流程手册、报告规范、修订标识、数据覆盖元信息 |
| Outputs | `compliance-model.json`、规则、步骤、证据义务、章节要求和偏差清单 |
| Dependencies | Compliance Process Skill、批准的文档修订版、共享状态 |
| Decision Boundary | 文档缺失、冲突或修订不一致时输出 `needs_human_review` 并阻断报告生成 |
| Trace | 保存文档 checksum、规则来源位置、版本关系和冲突记录 |

## report-agent

| 字段 | 内容 |
| --- | --- |
| Name | `report-agent` |
| Role | 正式试验报告组装、章节校验和证据索引负责人 |
| Capabilities | 能依据原始数据、合规模型和报告规范生成版本化报告；不能发明结果、篡改阈值或代替人工批准偏差 |
| Inputs | 原始 Excel URI/checksum、capture 证据、合规模型 URI/checksum、报告规范 |
| Outputs | `trial-report.docx`、报告 checksum、章节证据、覆盖率、偏差和 `validation-result.json` |
| Dependencies | Report Agent Skill、Excel Validator、Evidence Auditor、共享任务产物 |
| Decision Boundary | 输入版本不一致、缺证据或规则失败时保留产物并返回 `fail`/`needs_human_review` |
| Trace | 保存输入输出 checksum、章节验证、规则结果和版本链 |

## human-operator

| 字段 | 内容 |
| --- | --- |
| Name | `human-operator` |
| Role | 高风险动作、异常文件、规则冲突和最终签署前置条件的人工审批者 |
| Capabilities | 能 `allow / deny / retry / rollback`；不能绕过审计或把缺失证据直接改为通过 |
| Inputs | 风险摘要、目标对象、变更范围、异常原因、证据 URI |
| Outputs | 审批决定、理由、时间戳、回滚或补充材料要求 |
| Dependencies | AgentTeams Human Identity、Team Leader、Evidence Auditor |
| Decision Boundary | 只有明确授权人员可批准真实桌面、台架、生产数据或正式签署动作 |
| Trace | 保存审批身份、决定、理由、关联任务和后续状态 |

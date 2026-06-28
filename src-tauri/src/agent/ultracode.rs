pub fn overlay(task: &str) -> String {
    let task_line = if task.trim().is_empty() {
        "用户只请求启用 Ultracode 模式，尚未给出具体任务。"
    } else {
        task.trim()
    };

    format!(
        "Ultracode 多 Agent 编排模式已由用户显式启用。\n\
         当前任务：{task_line}\n\n\
         运行原则：\n\
         1. 你仍然是唯一可以改文件、跑 shell、做最终回复的主 Agent；子 Agent 只读、只做探索/审查/验证。\n\
         2. 对复杂任务，优先用 agent_spawn 派生 2-3 个互补视角，例如 Explorer、Planner、Reviewer、Verifier、Critic。\n\
         3. 子 Agent 适合并行阅读代码、找相似实现、做风险审查、反驳方案、检查遗漏；不要让子 Agent 修改文件或代替你做最终判断。\n\
         4. 默认并发心智上限为 3 个子任务；如果问题很小，直接完成，不要为了编排而编排。\n\
         5. 合成子 Agent 结果后，再由主 Agent 制定实施步骤、做代码改动、运行验证。\n\
         6. 对高风险改动使用 adversarial verify：至少派一个 Reviewer/Critic 专门找 bug、遗漏测试和上下文误读。\n\
         7. 长任务可以使用 context_inspect 查看上下文压力，必要时用 context_collapse 折叠旧消息。\n\
         8. 低频工具不一定直接出现在 tools schema 中；需要截图、OCR、打开路径等能力时，先用 tool_search，再用 execute_tool 执行发现的 deferred tool。\n\
         9. 需要隔离大改动或实验分支时，先用 worktree_create 创建独立 git worktree，再在结果中明确路径转换。\n\
         10. 输出给用户时只汇报重要结论、改动和验证结果，不暴露冗长内部编排过程。\n\n\
         上下文工程约定：\n\
         - agent_spawn 会继承项目人格、长期记忆、会话摘要和最近消息摘录，但只暴露只读工具 schema。\n\
         - 对探索型子任务用 context_mode=brief；对需要理解刚才长对话的复核用 context_mode=recent；对需要最大化父上下文继承和 placeholder 修复的任务用 context_mode=fork。\n\
         - /ultracode 会写 workflow journal；用户可用 /workflows 查看 run，用 /workflow resume <run_id> 恢复。\n\
         - 大型任务先用子 Agent 分面压缩事实，再把主上下文保持在计划、决策和变更证据上。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_mentions_agent_spawn() {
        let text = overlay("重构 agent");
        assert!(text.contains("agent_spawn"));
        assert!(text.contains("重构 agent"));
    }
}

//! 组件 8：上下文管理。历史超阈值时：先砍老的工具输出，再丢更老的回合。
//! 这直接决定全天聊天的 API 账单，不是可选项。MVP 用「估算字符数」近似 token。
use super::budget;
use super::conversation::Message;

fn est_chars(m: &Message) -> usize {
    let mut n = m.content.as_deref().map(|s| s.len()).unwrap_or(0);
    if let Some(tcs) = &m.tool_calls {
        for tc in tcs {
            n += tc.function.name.len() + tc.function.arguments.len() + 16;
        }
    }
    n + 8 // 角色等固定开销
}

fn total_chars(msgs: &[Message]) -> usize {
    msgs.iter().map(est_chars).sum()
}

/// 就地裁剪历史，使其大致控制在 max_chars 以内。
/// 返回第二阶段被整条移除的旧消息，供 rolling summary 消化。
/// 阶段一：把「较老的」工具结果内容截断（保留最近 KEEP_RECENT 条不动）。
/// 阶段二：仍超限则从最旧开始整条丢弃；丢完后若开头变成孤儿 tool 结果，一并丢掉，
///        避免给 API 发送没有对应 assistant.tool_calls 的 tool 消息（会 400）。
pub fn trim_collect_removed(msgs: &mut Vec<Message>, max_chars: usize) -> Vec<Message> {
    const KEEP_RECENT: usize = 8;
    const TOOL_KEEP: usize = 400;
    let mut removed = Vec::new();

    if total_chars(msgs) <= max_chars {
        return removed;
    }

    // 阶段一：截断老的、超长的 tool 结果
    if msgs.len() > KEEP_RECENT {
        let cut = msgs.len() - KEEP_RECENT;
        for m in msgs.iter_mut().take(cut) {
            if m.role == "tool" {
                if let Some(c) = &m.content {
                    if c.len() > TOOL_KEEP {
                        let head: String = c.chars().take(TOOL_KEEP).collect();
                        m.content = Some(format!("{head}…[已截断更早的工具输出]"));
                    }
                }
            }
        }
    }

    // 阶段二：从最旧丢起
    while total_chars(msgs) > max_chars && msgs.len() > 2 {
        removed.push(msgs.remove(0));
        // 别让开头留下孤儿 tool 结果
        while matches!(msgs.first(), Some(m) if m.role == "tool") {
            removed.push(msgs.remove(0));
        }
    }

    removed
}

/// 就地裁剪历史，使其大致控制在 max_chars 以内。
#[allow(dead_code)]
pub fn trim(msgs: &mut Vec<Message>, max_chars: usize) {
    let _ = trim_collect_removed(msgs, max_chars);
}

fn total_tokens(msgs: &[Message]) -> usize {
    budget::estimate_messages_tokens(msgs)
}

/// 按启发式 token 预算裁剪历史，返回被整条移除的旧消息。
pub fn trim_collect_removed_by_tokens(msgs: &mut Vec<Message>, max_tokens: usize) -> Vec<Message> {
    const KEEP_RECENT: usize = 8;
    const TOOL_KEEP: usize = 400;
    let mut removed = Vec::new();

    if total_tokens(msgs) <= max_tokens {
        return removed;
    }

    if msgs.len() > KEEP_RECENT {
        let cut = msgs.len() - KEEP_RECENT;
        for m in msgs.iter_mut().take(cut) {
            if m.role == "tool" {
                if let Some(c) = &m.content {
                    if c.len() > TOOL_KEEP {
                        let head: String = c.chars().take(TOOL_KEEP).collect();
                        m.content = Some(format!("{head}…[已截断更早的工具输出]"));
                    }
                }
            }
        }
    }

    while total_tokens(msgs) > max_tokens && msgs.len() > 2 {
        removed.push(msgs.remove(0));
        while matches!(msgs.first(), Some(m) if m.role == "tool") {
            removed.push(msgs.remove(0));
        }
    }

    removed
}

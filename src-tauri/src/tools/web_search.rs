//! web_search：DuckDuckGo Instant Answer JSON API（真实、免密钥）。
//! 注意：该 API 偏「即时答案 + 相关主题」，部分查询会返回空——那是真实的「无结果」，不是 mock。
use serde_json::Value;

pub async fn run(state: &crate::AppState, args: Value) -> Result<String, String> {
    let query = args["query"].as_str().ok_or("缺少参数 query")?.trim();
    if query.is_empty() {
        return Err("query 不能为空".to_string());
    }

    let resp = state
        .http
        .get("https://api.duckduckgo.com/")
        .query(&[
            ("q", query),
            ("format", "json"),
            ("no_html", "1"),
            ("no_redirect", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await
        .map_err(|e| format!("搜索请求失败：{e}"))?;

    if !resp.status().is_success() {
        return Err(format!("搜索返回 HTTP {}", resp.status()));
    }
    let v: Value = resp.json().await.map_err(|e| format!("解析搜索结果失败：{e}"))?;

    let mut out = String::new();

    if let Some(abs) = v["AbstractText"].as_str() {
        if !abs.is_empty() {
            out.push_str("摘要：");
            out.push_str(abs);
            if let Some(url) = v["AbstractURL"].as_str() {
                if !url.is_empty() {
                    out.push_str(&format!("\n来源：{url}"));
                }
            }
            out.push_str("\n\n");
        }
    }
    if let Some(ans) = v["Answer"].as_str() {
        if !ans.is_empty() {
            out.push_str(&format!("直接答案：{ans}\n\n"));
        }
    }

    // RelatedTopics 可能是「主题」或「分组（含 Topics 子数组）」，做一层展平
    let mut related: Vec<String> = Vec::new();
    if let Some(items) = v["RelatedTopics"].as_array() {
        for it in items {
            collect_topic(it, &mut related);
            if related.len() >= 6 {
                break;
            }
        }
    }
    if !related.is_empty() {
        out.push_str("相关结果：\n");
        for (i, r) in related.iter().take(6).enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, r));
        }
    }

    if out.trim().is_empty() {
        Ok(format!("「{query}」没有可用的即时答案或相关结果。"))
    } else {
        Ok(out.trim().to_string())
    }
}

fn collect_topic(it: &Value, out: &mut Vec<String>) {
    if let Some(text) = it["Text"].as_str() {
        if !text.is_empty() {
            let url = it["FirstURL"].as_str().unwrap_or("");
            if url.is_empty() {
                out.push(text.to_string());
            } else {
                out.push(format!("{text} （{url}）"));
            }
            return;
        }
    }
    // 分组：递归其 Topics
    if let Some(sub) = it["Topics"].as_array() {
        for t in sub {
            collect_topic(t, out);
            if out.len() >= 6 {
                return;
            }
        }
    }
}

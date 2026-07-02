//! Lorebook 召回与索引：BM25 稀疏检索 + 向量稠密检索 + RRF 混合融合。
//!
//! 跨模块依赖：
//! - `use crate::embed::{self, EmbeddingProvider}` 调用嵌入服务与 `rrf_fuse` / `cosine` 工具。
//! - `use super::manifest::{...}` 复用清单读写、缓存类型（LoreChunk / LoreIndexCache ...）、
//!   分块纯函数（split_lore_markdown / parse_markdown_meta / query_terms ...）。
//!
//! 公开 API（通过 `mod.rs` 的 `pub use` 重导出）：
//! `lorebook_context` / `lorebook_recall_detail` / `lorebook_rebuild_index` / `lorebook_index_status`。
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::embed;

use super::manifest::{
    char_count, collect_lore_sources, is_cjk, lore_file_signatures, merge_unique,
    normalize_search_text, pack_dir, parse_markdown_meta, query_terms, read_manifest_no_avatar,
    resolve_pack_file, search_tokens, split_lore_markdown, LoreChunk, LoreHit, LoreHitDetail,
    LoreIndexCache, LoreIndexStatus, LoreRecallDetail, LoreSearchStats, PackManifest,
    MAX_LORE_CONTEXT_CHARS, MAX_LORE_CONTEXT_CHUNKS, LORE_INDEX_VERSION,
};

pub fn lorebook_context(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
    query: Option<&str>,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> String {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    let Ok((mut chunks, cache_path)) = load_lore_index_with_cache(packs_dir, data_dir, id) else {
        return String::new();
    };
    if let Some(p) = provider {
        let model_key = format!("{}:{}", p.name(), p.dims());
        ensure_chunk_embeddings(&mut chunks, p, &model_key, &cache_path);
        render_lore_hits(select_lore_hits(&chunks, query, Some(p), hybrid_weight))
    } else {
        render_lore_hits(select_lore_hits(&chunks, query, None, hybrid_weight))
    }
}

/// Lorebook 索引状态：缓存是否存在、文件数、chunk 数、是否过期。
pub fn lorebook_index_status(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<LoreIndexStatus, String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_no_avatar(&dir)?;
    let cache_path = lore_index_cache_path(data_dir, id);
    let mut status = LoreIndexStatus {
        pack_id: id.to_string(),
        cache_exists: cache_path.exists(),
        version: None,
        file_count: 0,
        chunk_count: 0,
        files_stale: false,
        last_built_ms: 0,
    };
    if manifest.lorebook.is_empty() {
        return Ok(status);
    }
    let current_files = lore_file_signatures(&dir, &manifest)?;
    status.file_count = current_files.len();
    let last_built_ms = || {
        fs::metadata(&cache_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    };
    if let Ok(text) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<LoreIndexCache>(&text) {
            status.version = Some(cache.version);
            status.chunk_count = cache.chunks.len();
            status.files_stale = cache.version != LORE_INDEX_VERSION
                || cache.pack_id != id
                || cache.files != current_files;
            status.last_built_ms = last_built_ms();
        }
    } else {
        status.files_stale = !current_files.is_empty();
    }
    Ok(status)
}

/// 召回详情：全量打分（按 score 降序，截断到 limit），每条含命中关键词。
pub fn lorebook_recall_detail(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
    query: &str,
    limit: usize,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> Result<LoreRecallDetail, String> {
    let (mut chunks, cache_path) = load_lore_index_with_cache(packs_dir, data_dir, id)?;
    let total = chunks.len();
    if let Some(p) = provider {
        let model_key = format!("{}:{}", p.name(), p.dims());
        ensure_chunk_embeddings(&mut chunks, p, &model_key, &cache_path);
    }
    let terms = query_terms(query);
    let norm = normalize_search_text(query);
    let mut hits = score_all_lore_hits(&chunks, query, provider, hybrid_weight);
    if limit > 0 {
        hits.truncate(limit);
    }
    let details = hits
        .iter()
        .map(|hit| {
            let matched = matched_terms_for(&hit.chunk, &norm, &terms);
            LoreHitDetail {
                score: hit.score,
                source: hit.chunk.source.clone(),
                title: hit.chunk.title.clone(),
                heading: hit.chunk.heading.clone(),
                chunk_index: hit.chunk.chunk_index,
                text: hit.chunk.text.clone(),
                tags: hit.chunk.tags.clone(),
                keywords: hit.chunk.keywords.clone(),
                priority: hit.chunk.priority,
                matched_terms: matched,
                dense_score: hit.dense_score,
            }
        })
        .collect();
    Ok(LoreRecallDetail {
        query: query.to_string(),
        total_chunks: total,
        hits: details,
    })
}

/// 删除缓存并重建索引，返回新状态。
pub fn lorebook_rebuild_index(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<LoreIndexStatus, String> {
    let cache_path = lore_index_cache_path(data_dir, id);
    let _ = fs::remove_file(&cache_path);
    let _ = load_lore_index(packs_dir, data_dir, id)?;
    lorebook_index_status(packs_dir, data_dir, id)
}

fn load_lore_index(packs_dir: &Path, data_dir: &Path, id: &str) -> Result<Vec<LoreChunk>, String> {
    Ok(load_lore_index_with_cache(packs_dir, data_dir, id)?.0)
}

/// 加载 lorebook 索引，返回 chunks 与缓存文件路径（供 embedding 持久化使用）。
fn load_lore_index_with_cache(
    packs_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<(Vec<LoreChunk>, PathBuf), String> {
    let dir = pack_dir(packs_dir, id);
    let manifest = read_manifest_no_avatar(&dir)?;
    let cache_path = lore_index_cache_path(data_dir, id);
    if manifest.lorebook.is_empty() {
        return Ok((Vec::new(), cache_path));
    }
    let files = lore_file_signatures(&dir, &manifest)?;
    if let Ok(text) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<LoreIndexCache>(&text) {
            if cache.version == LORE_INDEX_VERSION && cache.pack_id == id && cache.files == files {
                return Ok((cache.chunks, cache_path));
            }
        }
    }

    let chunks = build_lore_chunks(&dir, &manifest)?;
    let cache = LoreIndexCache {
        version: LORE_INDEX_VERSION,
        pack_id: id.to_string(),
        files,
        chunks: chunks.clone(),
        embedding_model: None,
    };
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(&cache_path, format!("{text}\n"));
    }
    Ok((chunks, cache_path))
}

fn build_lore_chunks(dir: &Path, manifest: &PackManifest) -> Result<Vec<LoreChunk>, String> {
    let mut chunks = Vec::new();
    for source in collect_lore_sources(dir, manifest)? {
        let path = resolve_pack_file(dir, &source.rel_path, "lorebook.path")?;
        let text = fs::read_to_string(&path).map_err(|e| format!("读取 lore 失败：{e}"))?;
        let meta = parse_markdown_meta(&text);
        let title = meta
            .title
            .clone()
            .or_else(|| source.title.clone())
            .unwrap_or_else(|| source.rel_path.clone());
        let tags = merge_unique(source.tags.clone(), meta.tags.clone());
        let keywords = merge_unique(tags.clone(), meta.keywords.clone());
        let priority = meta.priority.or(source.priority).unwrap_or(0.0);
        split_lore_markdown(
            &mut chunks,
            &source.rel_path,
            title,
            tags,
            keywords,
            priority,
            &meta.body,
        );
    }
    Ok(chunks)
}

/// 为缺 embedding 的 chunk 批量计算向量并回写缓存。provider/model 变化时整体重算。
fn ensure_chunk_embeddings(
    chunks: &mut [LoreChunk],
    provider: &dyn embed::EmbeddingProvider,
    model_key: &str,
    cache_path: &Path,
) {
    let existing: Option<LoreIndexCache> = fs::read_to_string(cache_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let model_changed =
        existing.as_ref().and_then(|c| c.embedding_model.as_deref()) != Some(model_key);
    if model_changed {
        for chunk in chunks.iter_mut() {
            chunk.embedding = None;
        }
    }
    let to_embed: Vec<usize> = chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.embedding.is_none())
        .map(|(i, _)| i)
        .collect();
    if to_embed.is_empty() {
        return;
    }
    let texts: Vec<&str> = to_embed.iter().map(|&i| chunks[i].text.as_str()).collect();
    if let Ok(vectors) = provider.embed(&texts) {
        for (idx, vec) in to_embed.into_iter().zip(vectors.into_iter()) {
            chunks[idx].embedding = Some(vec);
        }
    }
    // 回写缓存（保留原 metadata，更新 chunks 与 embedding_model）
    let (version, pack_id, files) = existing
        .as_ref()
        .map(|c| (c.version, c.pack_id.clone(), c.files.clone()))
        .unwrap_or((LORE_INDEX_VERSION, String::new(), Vec::new()));
    let cache = LoreIndexCache {
        version,
        pack_id,
        files,
        chunks: chunks.to_vec(),
        embedding_model: Some(model_key.to_string()),
    };
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(cache_path, format!("{text}\n"));
    }
}

fn lore_index_cache_path(data_dir: &Path, id: &str) -> PathBuf {
    let safe_id = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    data_dir
        .join("lorebook-index")
        .join(format!("{safe_id}.json"))
}

fn score_all_lore_hits(
    chunks: &[LoreChunk],
    query: &str,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> Vec<LoreHit> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let norm = normalize_search_text(query);
    let stats = lore_search_stats(chunks);

    // 稀疏分（BM25 + 短语/元数据加权）
    let sparse: Vec<f32> = chunks
        .iter()
        .map(|chunk| score_lore_chunk(chunk, &norm, &terms, &stats))
        .collect();

    // 稠密分（余弦相似度）。provider 不可用或 chunk 缺 embedding 时为 None。
    let dense: Vec<Option<f32>> = if let Some(p) = provider {
        match p.embed(&[query]) {
            Ok(vectors) => {
                let q = vectors.into_iter().next().unwrap_or_default();
                chunks
                    .iter()
                    .map(|chunk| chunk.embedding.as_ref().map(|e| embed::cosine(e, &q)))
                    .collect()
            }
            Err(_) => chunks.iter().map(|_| None).collect(),
        }
    } else {
        chunks.iter().map(|_| None).collect()
    };

    let has_dense = dense.iter().any(|d| d.is_some());
    if !has_dense {
        // 纯稀疏路径：保持原语义，仅 score>0 入选
        let mut hits: Vec<LoreHit> = chunks
            .iter()
            .zip(sparse.iter())
            .filter_map(|(chunk, &score)| {
                (score > 0.0).then(|| LoreHit {
                    score,
                    chunk: chunk.clone(),
                    dense_score: None,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.chunk
                        .priority
                        .partial_cmp(&a.chunk.priority)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.chunk.source.cmp(&b.chunk.source))
                .then_with(|| a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
        });
        return hits;
    }

    // 混合路径：RRF 融合 sparse/dense 排名，按 hybrid_weight 加权
    let n = chunks.len();
    let mut sparse_order = (0..n).collect::<Vec<_>>();
    sparse_order.sort_by(|&a, &b| {
        sparse[b]
            .partial_cmp(&sparse[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut dense_order = (0..n).collect::<Vec<_>>();
    dense_order.sort_by(|&a, &b| {
        dense[b]
            .unwrap_or(-1.0)
            .partial_cmp(&dense[a].unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let sparse_rank = {
        let mut r = vec![usize::MAX; n];
        for (i, &idx) in sparse_order.iter().enumerate() {
            r[idx] = i;
        }
        r
    };
    let dense_rank = {
        let mut r = vec![usize::MAX; n];
        for (i, &idx) in dense_order.iter().enumerate() {
            r[idx] = i;
        }
        r
    };
    let w = hybrid_weight.clamp(0.0, 1.0);
    let fused = embed::rrf_fuse(&sparse_rank, &dense_rank, 60, w);
    let mut hits: Vec<LoreHit> = chunks
        .iter()
        .enumerate()
        .filter_map(|(i, chunk)| {
            // 至少一路上榜，或稀疏分>0，才算命中
            let has_signal =
                sparse_rank[i] != usize::MAX || dense_rank[i] != usize::MAX || sparse[i] > 0.0;
            if !has_signal {
                return None;
            }
            Some(LoreHit {
                score: fused[i],
                chunk: chunk.clone(),
                dense_score: dense[i],
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.dense_score
                    .unwrap_or(-1.0)
                    .partial_cmp(&a.dense_score.unwrap_or(-1.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.chunk.source.cmp(&b.chunk.source))
            .then_with(|| a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
    });
    hits
}

fn select_lore_hits(
    chunks: &[LoreChunk],
    query: &str,
    provider: Option<&dyn embed::EmbeddingProvider>,
    hybrid_weight: f32,
) -> Vec<LoreHit> {
    let mut hits = score_all_lore_hits(chunks, query, provider, hybrid_weight);
    hits.truncate(MAX_LORE_CONTEXT_CHUNKS);
    hits
}

fn score_lore_chunk(
    chunk: &LoreChunk,
    query: &str,
    terms: &[String],
    stats: &LoreSearchStats,
) -> f32 {
    let title = normalize_search_text(&chunk.title);
    let heading = normalize_search_text(chunk.heading.as_deref().unwrap_or_default());
    let tags = normalize_search_text(&chunk.tags.join(" "));
    let keywords = normalize_search_text(&chunk.keywords.join(" "));
    let text = normalize_search_text(&chunk.text);
    let mut metadata_score = 0.0;
    let mut chunk_score = 0.0;
    if !query.is_empty() && text.contains(query) {
        chunk_score += 12.0;
    }
    for term in terms {
        if title.contains(term) {
            metadata_score += 8.0;
        }
        if heading.contains(term) {
            chunk_score += 6.0;
        }
        if tags.contains(term) || keywords.contains(term) {
            metadata_score += 5.0;
        }
        if text.contains(term) {
            chunk_score += 2.0;
        }
    }
    if chunk_score > 0.0 {
        chunk_score
            + metadata_score
            + bm25_score(chunk, terms, stats)
            + chunk.priority.max(0.0) * 4.0
    } else if metadata_score > 0.0 && chunk.chunk_index == 0 {
        metadata_score + chunk.priority.max(0.0) * 4.0
    } else {
        0.0
    }
}

fn lore_search_stats(chunks: &[LoreChunk]) -> LoreSearchStats {
    let mut document_frequency = HashMap::new();
    let mut total_len = 0usize;
    for chunk in chunks {
        let terms = chunk_search_terms(chunk);
        total_len = total_len.saturating_add(terms.len());
        let unique = terms.into_iter().collect::<HashSet<_>>();
        for term in unique {
            *document_frequency.entry(term).or_insert(0) += 1;
        }
    }
    let document_count = chunks.len();
    let average_len = if document_count == 0 {
        1.0
    } else {
        (total_len as f32 / document_count as f32).max(1.0)
    };
    LoreSearchStats {
        document_count,
        average_len,
        document_frequency,
    }
}

fn bm25_score(chunk: &LoreChunk, query_terms: &[String], stats: &LoreSearchStats) -> f32 {
    if stats.document_count == 0 {
        return 0.0;
    }
    let terms = chunk_search_terms(chunk);
    if terms.is_empty() {
        return 0.0;
    }
    let mut tf = HashMap::new();
    for term in &terms {
        *tf.entry(term.as_str()).or_insert(0usize) += 1;
    }
    let doc_len = terms.len() as f32;
    let k1 = 1.2f32;
    let b = 0.75f32;
    let mut score = 0.0;
    for term in query_terms {
        let Some(freq) = tf.get(term.as_str()).copied() else {
            continue;
        };
        let df = stats.document_frequency.get(term).copied().unwrap_or(0) as f32;
        let n = stats.document_count as f32;
        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);
        let freq = freq as f32;
        let denom = freq + k1 * (1.0 - b + b * doc_len / stats.average_len);
        score += idf * (freq * (k1 + 1.0)) / denom;
    }
    score * 3.0
}

fn chunk_search_terms(chunk: &LoreChunk) -> Vec<String> {
    let haystack = [
        chunk.title.as_str(),
        chunk.heading.as_deref().unwrap_or_default(),
        &chunk.tags.join(" "),
        &chunk.keywords.join(" "),
        chunk.text.as_str(),
    ]
    .join(" ");
    let normalized = normalize_search_text(&haystack);
    let mut terms = search_tokens(&normalized);
    for token in search_tokens(&normalized) {
        if token.chars().any(is_cjk) {
            let chars = token.chars().collect::<Vec<_>>();
            for n in [2usize, 3] {
                if chars.len() >= n {
                    for window in chars.windows(n) {
                        terms.push(window.iter().collect::<String>());
                    }
                }
            }
        }
    }
    terms
}

fn matched_terms_for(chunk: &LoreChunk, query: &str, terms: &[String]) -> Vec<String> {
    let title = normalize_search_text(&chunk.title);
    let heading = normalize_search_text(chunk.heading.as_deref().unwrap_or_default());
    let tags = normalize_search_text(&chunk.tags.join(" "));
    let keywords = normalize_search_text(&chunk.keywords.join(" "));
    let text = normalize_search_text(&chunk.text);
    let mut matched = Vec::new();
    if !query.is_empty() && text.contains(query) {
        matched.push(query.to_string());
    }
    for term in terms {
        if title.contains(term)
            || heading.contains(term)
            || tags.contains(term)
            || keywords.contains(term)
            || text.contains(term)
        {
            matched.push(term.clone());
        }
    }
    matched
}

fn render_lore_hits(hits: Vec<LoreHit>) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Use these retrieved lore snippets as supporting facts only. Character Card, persona, speech style, and OOC rules remain authoritative.\n",
    );
    for hit in hits {
        let chunk = hit.chunk;
        let heading = chunk
            .heading
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&chunk.title);
        let tags = if chunk.tags.is_empty() {
            String::new()
        } else {
            format!("; tags={}", chunk.tags.join(", "))
        };
        let block = format!(
            "\n## {} / {}\nsource: {}#{}; score={:.1}{}\n{}\n",
            chunk.title,
            heading,
            chunk.source,
            chunk.chunk_index,
            hit.score,
            tags,
            chunk.text.trim()
        );
        if char_count(&out).saturating_add(char_count(&block)) > MAX_LORE_CONTEXT_CHARS {
            break;
        }
        out.push_str(&block);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "demiurge-pack-{label}-{}",
            crate::store::new_session_id()
        ))
    }

    #[test]
    fn lorebook_context_retrieves_markdown_chunks_and_caches_index() {
        let packs = temp_dir("lorebook");
        let data = temp_dir("lorebook_data");
        let dir = packs.join("demo");
        fs::create_dir_all(dir.join("lore").join("arc")).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(dir.join("persona.md"), "Base persona.").unwrap();
        fs::write(
            dir.join("manifest.json"),
            r#"{
  "schema_version": "2.0",
  "id": "demo",
  "name": "Demo",
  "persona": "persona.md",
  "lorebook": [
    { "path": "lore", "title": "故事设定", "tags": ["月亮城"], "recursive": true, "extensions": ["md", "txt"], "priority": 0.5 }
  ]
}"#,
        )
        .unwrap();
        fs::write(
            dir.join("lore").join("story.md"),
            r#"---
title: 月亮城年表
tags: [剧情, 城市]
keywords: [银钟塔, 夜巡]
priority: 0.8
---

# 银钟塔事件

月亮城的银钟塔在雨夜停摆，夜巡队随后封锁了旧桥。

# 海边集市

这里记录的是海边集市和甜点摊的日常。"#,
        )
        .unwrap();
        fs::write(
            dir.join("lore").join("arc").join("battle.txt"),
            "赤桥战役发生在第三章，夜巡队在这里第一次公开协助主角。",
        )
        .unwrap();

        let context = lorebook_context(&packs, &data, "demo", Some("银钟塔后来怎么了"), None, 0.5);
        assert!(context.contains("retrieved lore snippets"));
        assert!(context.contains("月亮城年表"));
        assert!(context.contains("银钟塔在雨夜停摆"));
        assert!(!context.contains("甜点摊"));
        assert!(lore_index_cache_path(&data, "demo").exists());

        let nested = lorebook_context(&packs, &data, "demo", Some("赤桥战役在哪一章"), None, 0.5);
        assert!(nested.contains("赤桥战役发生在第三章"));

        let _ = fs::remove_dir_all(packs);
        let _ = fs::remove_dir_all(data);
    }
}


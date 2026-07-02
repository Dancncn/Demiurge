# Lorebook 向量召回与混合 RAG

> 本篇描述 Demiurge Lorebook 检索的**稀疏 + 稠密混合召回**实现。面向已读过 [14-角色包系统](14-pack-system.md) 与 [03-上下文工程](03-context-engineering.md) 的协作者。

## 定位

Lorebook 是角色包内的本地知识库（`lore/*.md`/`.txt`），按用户输入检索后作为 `Retrieved Lorebook` 段注入 system prompt。原始实现是纯稀疏检索（短语匹配 + BM25 + 元数据加权）。本子系统在其上叠加**稠密向量召回**，用 RRF（Reciprocal Rank Fusion）融合两路排名，提升语义相关性，同时保留稀疏路对关键词与 CJK ngram 的精确命中。

设计原则：**远程优先、本地预留、回落安全**。未配置 embedding provider 时，整条链路退化为原纯 BM25，零行为变化。

## 数据流动

```
用户输入 query
      │
      ▼
prompt::build_with_report_for_input
  └─ embed::provider_from_settings(http, settings)   ← Option<Box<dyn EmbeddingProvider>>
      │  (None if embedding_enabled=false / provider="none" / 配置不全)
      ▼
lorebook_section ── lorebook_context(packs_dir, data_dir, id, query, provider, hybrid_weight)
      │
      ├─ load_lore_index_with_cache → (chunks, cache_path)
      │     └─ 缓存命中（version + pack_id + 文件签名一致）→ 直接复用 cache.chunks
      │     └─ 未命中 → build_lore_chunks 重建并写 cache
      │
      ├─ if provider: ensure_chunk_embeddings(chunks, provider, model_key, cache_path)
      │     ├─ 读 cache.embedding_model；与 model_key 不一致 → 清空所有 chunk.embedding
      │     ├─ 收集 embedding=None 的 chunk，批量 provider.embed(texts)
      │     └─ 回写 cache（chunks 含向量 + embedding_model=model_key）
      │
      └─ select_lore_hits(chunks, query, provider, hybrid_weight)
            ├─ sparse[i] = score_lore_chunk(...)         （BM25 + 短语/元数据加权）
            ├─ dense[i]  = provider.embed([query]) → cosine(query_vec, chunk.embedding)
            ├─ 无 dense 信号 → 纯稀疏 top-N（原语义）
            └─ 有 dense 信号 → RRF 融合两路排名 → 按 hybrid_weight 加权 → top-N
```

## 关键结构与位置

- `EmbeddingProvider` trait、`RemoteEmbeddingProvider`、`cosine`、`rrf_fuse`、`provider_from_settings`：`src-tauri/src/embed/mod.rs`。
- `LoreChunk.embedding: Option<Vec<f32>>`、`LoreIndexCache.embedding_model: Option<String>`：`src-tauri/src/pack/mod.rs`。两者均 `#[serde(default)]`，旧缓存可向后兼容解析。
- `score_all_lore_hits` / `select_lore_hits`：稀疏 + 稠密 + RRF 融合，`src-tauri/src/pack/mod.rs`。
- `ensure_chunk_embeddings` / `load_lore_index_with_cache`：向量懒计算与缓存回写，`src-tauri/src/pack/mod.rs`。
- `lorebook_context` / `lorebook_recall_detail`：provider + hybrid_weight 透传，`src-tauri/src/pack/mod.rs`。
- Settings 字段 `embedding_enabled` / `embedding_provider` / `embedding_base_url` / `embedding_api_key` / `embedding_model` / `embedding_dims` / `hybrid_weight`：`src-tauri/src/store/mod.rs`。
- 前端配置区：`src/components/SettingsDialog.tsx` 的 `settings.embedding.*`（context tab 内）。

## 为什么 embed 是同步的

`lorebook_context` 处于 prompt 组装热路径（`build_for_input` 每个 step 调一次），改 async 会波及 4 个 caller。`RemoteEmbeddingProvider::embed` 因此在**独立 OS 线程 + 临时 current-thread tokio runtime** 上阻塞调用异步 reqwest，不依赖调用方 runtime 类型（multi-thread / current-thread 均可），也避免 `block_in_place` 在非 multi-thread runtime 上 panic的隐患。代价：每次 query embed 占用一个线程 ~100–300ms；chunk 向量批量计算仅发生在索引变更后（缓存命中后零开销）。

## RRF 融合语义

`rrf_fuse(sparse_ranks, dense_ranks, k=60, weight)` 对每个 chunk 计算：

```
score = (1-weight) * sparse_rrf + weight * dense_rrf
sparse_rrf = 1/(k + sparse_rank + 1)，未上榜为 0
dense_rrf  = 1/(k + dense_rank + 1)，未上榜为 0
```

- `weight=0` → 纯稀疏；`weight=1` → 纯稠密；`weight=0.5` → 均衡融合。
- 命中条件：至少一路上榜，或稀疏分>0。保证语义命中（dense-only）的 chunk 也能入选。
- 排序：融合分降序，再以 dense_score、source、chunk_index 作 tie-break。

## 缓存失效

`LoreIndexCache` 同时记录 `files`（文件签名：路径/大小/mtime）与 `embedding_model`。两层失效：

1. **文件签名不一致** → 整个 cache 作废，重建 chunks（向量随 chunk 重建回到 None）。
2. **embedding_model 变化**（provider 或 dims 切换）→ `ensure_chunk_embeddings` 清空所有 `chunk.embedding` 并重算，但保留 chunks 本身不重建。

## 后续扩展点（预留，未实现）

- **本地 fastembed**：cargo feature `embeddings-local`（`src-tauri/Cargo.toml`）。启用后编译 `LocalEmbeddingProvider` 桩，当前 `embed` 返回明确错误；后续接入 `fastembed` crate + BGE-small-zh 模型 + 模型下载 UX（复用 OCR 的可选下载范式）。默认不启用，避免 ONNX runtime 增 ~20–40MB 包体。
- **Cross-encoder reranker**：在 `select_lore_hits` 取 top-N 后、`render_lore_hits` 前插入 reranker 步骤；可用 `reranker`/`ort` crate 加载 MiniLM cross-encoder。当前未接入。
- **凭据管理**：`embedding_api_key` 目前存 settings（明文），后续应迁入 `credentials.rs` keyring，与 LLM API Key 一致。

## 验证

- `embed::tests`：cosine 基本向量、空/不匹配返回 0、RRF 融合排名、缺榜归零、weight 偏移。
- `pack::tests::lorebook_context_retrieves_markdown_chunks_and_caches_index`：provider=None 时纯稀疏路径与原行为一致。
- 桌面端手测：Settings > Context > 向量召回，配远程 provider 后 `/recall <query>` 应同时显示 `score=`（RRF 融合）与 `dense=`（余弦）。

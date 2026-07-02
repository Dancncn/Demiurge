//! Embedding provider 抽象，用于 Lorebook 混合召回（BM25 + dense + RRF）。
//!
//! 远程优先：`RemoteEmbeddingProvider` 调 OpenAI 兼容 `/v1/embeddings`。
//! 本地 fastembed 通过 `embeddings-local` cargo feature 预留接口，默认不编译、不打包 ONNX runtime。
//!
//! `embed` 是同步的——远程实现内部用独立线程 + 临时 current-thread runtime 阻塞调用异步 HTTP，
//! 避免污染 tokio 热路径的 async 链（`lorebook_context` / prompt 组装保持同步）。
use serde::Deserialize;

/// 向量召回 provider 抽象。
pub trait EmbeddingProvider: Send + Sync {
    /// provider 标识（"remote" / "local"），用于缓存失效判断。
    fn name(&self) -> &str;
    /// 输出向量维度。
    fn dims(&self) -> usize;
    /// 批量 embed，返回与输入等长的向量列表。
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String>;
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

/// OpenAI 兼容 `/v1/embeddings` 远程 provider（DashScope text-embedding-v3 / OpenAI text-embedding-3-small 等）。
pub struct RemoteEmbeddingProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    dims: usize,
}

impl RemoteEmbeddingProvider {
    pub fn new(
        http: reqwest::Client,
        base_url: String,
        api_key: String,
        model: String,
        dims: usize,
    ) -> Self {
        Self {
            http,
            base_url,
            api_key,
            model,
            dims,
        }
    }
}

impl EmbeddingProvider for RemoteEmbeddingProvider {
    fn name(&self) -> &str {
        "remote"
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // 在独立线程 + 临时 current-thread runtime 上阻塞调用异步 HTTP，
        // 不依赖调用方所处的 tokio runtime 类型（multi-thread / current-thread 均可）。
        let http = self.http.clone();
        let base_url = self.base_url.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let inputs: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<Vec<f32>>, String>>();
        std::thread::spawn(move || {
            let result = (|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("创建 embedding runtime 失败：{e}"))?;
                rt.block_on(async move {
                    let url = format!("{}/v1/embeddings", base_url.trim_end_matches('/'));
                    let body = serde_json::json!({ "model": model, "input": inputs });
                    let resp = http
                        .post(&url)
                        .bearer_auth(&api_key)
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| format!("embedding 请求失败：{e}"))?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_default();
                        return Err(format!("embedding 返回 {status}: {text}"));
                    }
                    let parsed: EmbeddingResponse = resp
                        .json()
                        .await
                        .map_err(|e| format!("解析 embedding 响应失败：{e}"))?;
                    Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
                })
            })();
            let _ = tx.send(result);
        });
        rx.recv().map_err(|e| format!("embedding 线程异常：{e}"))?
    }
}

/// 根据设置构造 provider。None 表示未启用或配置不全。
pub fn provider_from_settings(
    http: &reqwest::Client,
    settings: &crate::store::Settings,
) -> Option<Box<dyn EmbeddingProvider>> {
    if !settings.embedding_enabled {
        return None;
    }
    match settings.embedding_provider.as_str() {
        "remote" => {
            let base = settings.embedding_base_url.trim();
            let key = settings.embedding_api_key.trim();
            let model = settings.embedding_model.trim();
            if base.is_empty() || key.is_empty() || model.is_empty() {
                return None;
            }
            Some(Box::new(RemoteEmbeddingProvider::new(
                http.clone(),
                base.to_string(),
                key.to_string(),
                model.to_string(),
                settings.embedding_dims,
            )))
        }
        #[cfg(feature = "embeddings-local")]
        "local" => Some(Box::new(LocalEmbeddingProvider::default())),
        // "none" / 未知：不启用向量召回，回落到纯 BM25。
        _ => None,
    }
}

/// 本地 embedding provider 桩（cargo feature `embeddings-local`）。
///
/// 默认不编译。启用后仍需后续接入 fastembed crate 与模型下载 UX 才能真正推理；
/// 当前调用会返回明确错误，便于在启用 feature 后早发现"未接模型"的情况。
#[cfg(feature = "embeddings-local")]
#[derive(Default)]
pub struct LocalEmbeddingProvider {
    /// 预留：模型路径 / 维度等，后续接入 fastembed 时填充。
    #[allow(dead_code)]
    model: String,
}

#[cfg(feature = "embeddings-local")]
impl EmbeddingProvider for LocalEmbeddingProvider {
    fn name(&self) -> &str {
        "local"
    }
    fn dims(&self) -> usize {
        0
    }
    fn embed(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        Err("本地 embedding 尚未接入 fastembed 模型；请改用 remote provider 或等待后续集成。"
            .to_string())
    }
}

/// 余弦相似度。任一为零向量时返回 0。
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_handles_basic_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine(&a, &b).abs() < 1e-6);
        let c = vec![1.0, 1.0, 0.0];
        // cos(45°) = 1/√2 ≈ 0.7071
        assert!(
            (cosine(&a, &c) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5,
            "got {}",
            cosine(&a, &c)
        );
    }

    #[test]
    fn cosine_returns_zero_for_mismatched_or_empty() {
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn rrf_fuses_sparse_and_dense_ranks() {
        // 3 chunks。sparse 排名 [0,1,2]，dense 排名 [2,0,1]。
        // chunk0: s_rank0 d_rank2；chunk1: s_rank1 d_rank0；chunk2: s_rank2 d_rank1。
        let sparse = vec![0usize, 1, 2];
        let dense = vec![2usize, 0, 1];
        let fused = rrf_fuse(&sparse, &dense, 60, 0.5);
        assert_eq!(fused.len(), 3);
        // weight=0.5：chunk1（dense 最强 + sparse 次弱）应最高，chunk2（双弱）应最低。
        assert!(fused[1] > fused[0], "chunk1 should beat chunk0");
        assert!(fused[0] > fused[2], "chunk0 should beat chunk2");
    }

    #[test]
    fn rrf_treats_missing_rank_as_zero() {
        // chunk0 只在 sparse 上榜（rank 0），chunk1 只在 dense 上榜（rank 0）。
        let sparse = vec![0usize, usize::MAX];
        let dense = vec![usize::MAX, 0];
        let fused = rrf_fuse(&sparse, &dense, 60, 0.5);
        // weight=0.5：两 chunk 各有一路 1/61，加权后都等于 0.5/61。
        assert!((fused[0] - 0.5 / 61.0).abs() < 1e-6);
        assert!((fused[1] - 0.5 / 61.0).abs() < 1e-6);
    }

    #[test]
    fn rrf_weight_biases_toward_chosen_channel() {
        let sparse = vec![0usize, 1];
        let dense = vec![1usize, 0];
        // weight=0 → 纯稀疏：chunk0（sparse rank 0）应更高
        let fused_sparse = rrf_fuse(&sparse, &dense, 60, 0.0);
        assert!(fused_sparse[0] > fused_sparse[1]);
        // weight=1 → 纯稠密：chunk1（dense rank 0）应更高
        let fused_dense = rrf_fuse(&sparse, &dense, 60, 1.0);
        assert!(fused_dense[1] > fused_dense[0]);
    }
}

/// RRF（Reciprocal Rank Fusion）融合：把每个 chunk 的 sparse 与 dense 排名融合成分数。
/// `ranks[i]` = chunk i 在该路排序中的名次（0-based），`usize::MAX` 表示未上榜。
/// `k` 通常取 60。`weight` 是 dense 的权重（0.0=纯稀疏，1.0=纯稠密，0.5=均衡）。
pub fn rrf_fuse(sparse_ranks: &[usize], dense_ranks: &[usize], k: usize, weight: f32) -> Vec<f32> {
    let w = weight.clamp(0.0, 1.0);
    sparse_ranks
        .iter()
        .zip(dense_ranks.iter())
        .map(|(s, d)| {
            let s_score = if *s == usize::MAX {
                0.0
            } else {
                1.0 / (k as f32 + *s as f32 + 1.0)
            };
            let d_score = if *d == usize::MAX {
                0.0
            } else {
                1.0 / (k as f32 + *d as f32 + 1.0)
            };
            s_score * (1.0 - w) + d_score * w
        })
        .collect()
}

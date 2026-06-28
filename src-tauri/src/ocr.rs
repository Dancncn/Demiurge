//! OCR 模型管理 + PP-OCRv5 推理。模型由用户按需下载到 app data，不随安装包内置。
use futures_util::StreamExt;
use image::DynamicImage;
use oar_ocr::oarocr::{OAROCRBuilder, OAROCR};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

pub const DET_FILE: &str = "pp-ocrv5_mobile_det.onnx";
pub const REC_FILE: &str = "pp-ocrv5_mobile_rec.onnx";
pub const DICT_FILE: &str = "ppocrv5_dict.txt";

#[derive(Default)]
pub struct OcrState {
    engine: Mutex<Option<OAROCR>>,
}

impl OcrState {
    pub fn clear(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            *engine = None;
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcrModelSource {
    #[serde(rename = "modelscope")]
    ModelScope,
    #[serde(rename = "huggingface")]
    HuggingFace,
}

impl OcrModelSource {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "huggingface" | "hugging_face" | "hf" => OcrModelSource::HuggingFace,
            _ => OcrModelSource::ModelScope,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            OcrModelSource::ModelScope => "modelscope",
            OcrModelSource::HuggingFace => "huggingface",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OcrModelSource::ModelScope => "ModelScope",
            OcrModelSource::HuggingFace => "Hugging Face",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            OcrModelSource::ModelScope => {
                "Recommended for mainland China. Uses the ModelScope mirror for PP-OCRv5 mobile files."
            }
            OcrModelSource::HuggingFace => {
                "Use this when Hugging Face is reachable from the current network."
            }
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            OcrModelSource::ModelScope => "https://modelscope.cn/models/greatv/oar-ocr",
            OcrModelSource::HuggingFace => "https://huggingface.co/monkt/paddleocr-onnx",
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrModelFileStatus {
    pub name: &'static str,
    pub present: bool,
    pub bytes: u64,
    pub download_url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrModelStatus {
    pub installed: bool,
    pub model_dir: String,
    pub source: String,
    pub source_label: String,
    pub source_note: String,
    pub source_url: String,
    pub files: Vec<OcrModelFileStatus>,
    pub missing: Vec<&'static str>,
    pub total_bytes: u64,
    pub manual_install_hint: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OcrDownloadEvent {
    source: String,
    source_label: String,
    file: &'static str,
    index: usize,
    total_files: usize,
    completed_files: usize,
    downloaded_bytes: u64,
    downloaded_total_bytes: u64,
    total_bytes: Option<u64>,
    phase: &'static str,
    url: String,
    done: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    pub conf: f32,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub frame_w: u32,
    pub frame_h: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrFrame {
    pub lines: Vec<OcrLine>,
    pub text: String,
}

struct ModelFile {
    target: &'static str,
    url: String,
}

pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("ocr").join("pp-ocrv5-mobile")
}

pub fn model_status(state: &crate::AppState) -> OcrModelStatus {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let settings = state.settings.lock().unwrap().clone();
    status_for_dir(
        &model_dir(&data_dir),
        OcrModelSource::from_setting(&settings.ocr_model_source),
    )
}

pub async fn download_models(
    app: AppHandle,
    state: &crate::AppState,
    source: OcrModelSource,
) -> Result<OcrModelStatus, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let dir = model_dir(&data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 OCR 模型目录失败：{e}"))?;

    let files = source_files(source);
    let mut completed_bytes = 0u64;
    for (idx, file) in files.iter().enumerate() {
        let bytes = download_one(
            &app,
            state,
            source,
            file,
            idx + 1,
            files.len(),
            &dir,
            completed_bytes,
        )
        .await?;
        completed_bytes += bytes;
    }
    state.ocr.clear();
    Ok(status_for_dir(&dir, source))
}

pub fn ensure_engine(state: &crate::AppState) -> Result<(), String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    let dir = model_dir(&data_dir);
    let status = status_for_dir(&dir, OcrModelSource::ModelScope);
    if !status.installed {
        return Err(format!(
            "OCR 模型未安装，缺少：{}。请先在设置里下载 PP-OCRv5 mobile 模型。",
            status.missing.join(", ")
        ));
    }

    let mut guard = state.ocr.engine.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(());
    }
    let engine = OAROCRBuilder::new(
        dir.join(DET_FILE).to_string_lossy().to_string(),
        dir.join(REC_FILE).to_string_lossy().to_string(),
        dir.join(DICT_FILE).to_string_lossy().to_string(),
    )
    .build()
    .map_err(|e| format!("OCR 引擎初始化失败：{e}"))?;
    *guard = Some(engine);
    Ok(())
}

pub fn recognize_rgba(state: &crate::AppState, rgba: image::RgbaImage) -> Result<OcrFrame, String> {
    ensure_engine(state)?;
    let mut guard = state.ocr.engine.lock().map_err(|e| e.to_string())?;
    let engine = guard.as_mut().ok_or("OCR 引擎未初始化")?;
    let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();
    let results = engine
        .predict(vec![rgb])
        .map_err(|e| format!("OCR 识别失败：{e}"))?;

    let mut lines = Vec::new();
    if let Some(result) = results.first() {
        if result.rectified_img.is_some() {
            return Err(
                "OCR 触发了文档矫正，截图坐标不可安全映射；请改用普通屏幕区域。".to_string(),
            );
        }
        let frame_w = result.input_img.width();
        let frame_h = result.input_img.height();
        for region in &result.text_regions {
            if let Some((text, conf)) = region.text_with_confidence() {
                let text = text.trim();
                if text.is_empty() || is_noise(text) {
                    continue;
                }
                let b = &region.bounding_box;
                let candidate = OcrLine {
                    text: text.to_string(),
                    conf,
                    x0: b.x_min(),
                    y0: b.y_min(),
                    x1: b.x_max(),
                    y1: b.y_max(),
                    frame_w,
                    frame_h,
                };
                if !lines.iter().any(|line: &OcrLine| {
                    line.text == candidate.text && boxes_overlap(line, &candidate)
                }) {
                    lines.push(candidate);
                }
            }
        }
    }

    lines.sort_by(|a, b| {
        let ay = (a.y0 / 8.0).floor() as i32;
        let by = (b.y0 / 8.0).floor() as i32;
        ay.cmp(&by)
            .then(a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal))
    });
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err("未识别到文本".to_string());
    }
    Ok(OcrFrame { lines, text })
}

fn status_for_dir(dir: &Path, source: OcrModelSource) -> OcrModelStatus {
    let source_files = source_files(source);
    let files = source_files
        .iter()
        .map(|file| {
            let path = dir.join(file.target);
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            OcrModelFileStatus {
                name: file.target,
                present: bytes > 0,
                bytes,
                download_url: file.url.clone(),
            }
        })
        .collect::<Vec<_>>();
    let missing = files
        .iter()
        .filter(|f| !f.present)
        .map(|f| f.name)
        .collect::<Vec<_>>();
    let total_bytes = files.iter().map(|f| f.bytes).sum();
    OcrModelStatus {
        installed: missing.is_empty(),
        model_dir: dir.display().to_string(),
        source: source.as_setting().to_string(),
        source_label: source.label().to_string(),
        source_note: source.note().to_string(),
        source_url: source.url().to_string(),
        files,
        missing,
        total_bytes,
        manual_install_hint: manual_install_hint(dir, source),
    }
}

fn manual_install_hint(dir: &Path, source: OcrModelSource) -> String {
    format!(
        "If download fails, download the missing files from {} and place them in {}.",
        source.label(),
        dir.display()
    )
}

fn source_files(source: OcrModelSource) -> Vec<ModelFile> {
    match source {
        OcrModelSource::ModelScope => [DET_FILE, REC_FILE, DICT_FILE]
            .into_iter()
            .map(|target| ModelFile {
                target,
                url: format!("https://modelscope.cn/models/greatv/oar-ocr/resolve/master/{target}"),
            })
            .collect(),
        OcrModelSource::HuggingFace => vec![
            ModelFile {
                target: DET_FILE,
                url: "https://huggingface.co/monkt/paddleocr-onnx/resolve/main/detection/v5/det.onnx".to_string(),
            },
            ModelFile {
                target: REC_FILE,
                url: "https://huggingface.co/monkt/paddleocr-onnx/resolve/main/languages/chinese/rec.onnx".to_string(),
            },
            ModelFile {
                target: DICT_FILE,
                url: "https://huggingface.co/monkt/paddleocr-onnx/resolve/main/languages/chinese/dict.txt".to_string(),
            },
        ],
    }
}

async fn download_one(
    app: &AppHandle,
    state: &crate::AppState,
    source: OcrModelSource,
    file: &ModelFile,
    index: usize,
    total_files: usize,
    dir: &Path,
    completed_bytes: u64,
) -> Result<u64, String> {
    let target = dir.join(file.target);
    let tmp = dir.join(format!("{}.download", file.target));
    emit_download_event(
        app,
        source,
        file,
        index,
        total_files,
        index.saturating_sub(1),
        0,
        completed_bytes,
        None,
        "starting",
        false,
    );
    let response = state
        .http
        .get(&file.url)
        .send()
        .await
        .map_err(|e| format!("下载 {} 失败：{e}", file.target))?
        .error_for_status()
        .map_err(|e| format!("下载 {} 返回错误状态：{e}", file.target))?;
    let total_bytes = response.content_length();
    let mut stream = response.bytes_stream();
    let mut out = std::fs::File::create(&tmp).map_err(|e| format!("创建临时文件失败：{e}"))?;
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取下载流失败：{e}"))?;
        out.write_all(&chunk)
            .map_err(|e| format!("写入模型文件失败：{e}"))?;
        downloaded += chunk.len() as u64;
        emit_download_event(
            app,
            source,
            file,
            index,
            total_files,
            index.saturating_sub(1),
            downloaded,
            completed_bytes + downloaded,
            total_bytes,
            "downloading",
            false,
        );
    }
    out.flush().map_err(|e| format!("刷新模型文件失败：{e}"))?;
    drop(out);
    if downloaded == 0 {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("下载 {} 得到空文件", file.target));
    }
    std::fs::rename(&tmp, &target).map_err(|e| format!("保存模型文件失败：{e}"))?;
    emit_download_event(
        app,
        source,
        file,
        index,
        total_files,
        index,
        downloaded,
        completed_bytes + downloaded,
        Some(downloaded),
        "finished",
        true,
    );
    Ok(downloaded)
}

#[allow(clippy::too_many_arguments)]
fn emit_download_event(
    app: &AppHandle,
    source: OcrModelSource,
    file: &ModelFile,
    index: usize,
    total_files: usize,
    completed_files: usize,
    downloaded_bytes: u64,
    downloaded_total_bytes: u64,
    total_bytes: Option<u64>,
    phase: &'static str,
    done: bool,
) {
    let _ = app.emit(
        "ocr-download-progress",
        OcrDownloadEvent {
            source: source.as_setting().to_string(),
            source_label: source.label().to_string(),
            file: file.target,
            index,
            total_files,
            completed_files,
            downloaded_bytes,
            downloaded_total_bytes,
            total_bytes,
            phase,
            url: file.url.clone(),
            done,
        },
    );
}

fn is_noise(s: &str) -> bool {
    s.chars().filter(|c| c.is_alphanumeric()).count() < 2
}

fn boxes_overlap(a: &OcrLine, b: &OcrLine) -> bool {
    let ix0 = a.x0.max(b.x0);
    let iy0 = a.y0.max(b.y0);
    let ix1 = a.x1.min(b.x1);
    let iy1 = a.y1.min(b.y1);
    let iw = (ix1 - ix0).max(0.0);
    let ih = (iy1 - iy0).max(0.0);
    let inter = iw * ih;
    let area_a = ((a.x1 - a.x0) * (a.y1 - a.y0)).max(1.0);
    let area_b = ((b.x1 - b.x0) * (b.y1 - b.y0)).max(1.0);
    inter / (area_a + area_b - inter).max(1.0) > 0.6
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn test_state(data_dir: PathBuf) -> crate::AppState {
        crate::AppState {
            http: reqwest::Client::new(),
            settings: Mutex::new(crate::store::Settings::default()),
            sessions: Mutex::new(crate::store::SessionStore::default()),
            pending_confirms: Mutex::new(std::collections::HashMap::new()),
            session_permission_rules: Mutex::new(std::collections::HashMap::new()),
            plan_state: Mutex::new(crate::PlanState::default()),
            edit_undo_stack: Mutex::new(Vec::new()),
            workflow_runs: Mutex::new(Vec::new()),
            workflow_cancels: Mutex::new(std::collections::HashMap::new()),
            session_engine: Mutex::new(crate::agent::session_engine::SessionEngineState::default()),
            mcp: crate::mcp::McpManager::default(),
            cancel: std::sync::atomic::AtomicBool::new(false),
            busy: std::sync::atomic::AtomicBool::new(false),
            data_dir: Mutex::new(data_dir),
            sandbox_dir: Mutex::new(PathBuf::new()),
            packs_dir: Mutex::new(PathBuf::new()),
            ocr: OcrState::default(),
        }
    }

    #[test]
    fn missing_models_are_reported() {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_ocr_status_{}",
            crate::store::new_session_id()
        ));
        let state = test_state(dir.clone());
        let status = model_status(&state);
        assert!(!status.installed);
        assert_eq!(status.missing.len(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_source_setting_accepts_aliases() {
        assert_eq!(
            OcrModelSource::from_setting("hf"),
            OcrModelSource::HuggingFace
        );
        assert_eq!(
            OcrModelSource::from_setting("modelscope"),
            OcrModelSource::ModelScope
        );
    }

    #[test]
    fn status_exposes_source_guidance_and_download_urls() {
        let dir = std::env::temp_dir().join(format!(
            "demiurge_ocr_guidance_{}",
            crate::store::new_session_id()
        ));
        let status = status_for_dir(&dir, OcrModelSource::ModelScope);
        assert_eq!(status.source, "modelscope");
        assert_eq!(status.source_label, "ModelScope");
        assert!(status.source_note.contains("mainland China"));
        assert!(status.source_url.contains("modelscope.cn"));
        assert!(status.manual_install_hint.contains(&status.model_dir));
        assert_eq!(status.files.len(), 3);
        assert!(status
            .files
            .iter()
            .all(|file| file.download_url.starts_with("https://")));
    }
}

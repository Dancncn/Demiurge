//! screen：只读屏幕感知工具。截图写入沙盒，避免把图片数据灌入上下文。
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CAPTURE_SIDE: u32 = 8192;
const MAX_CAPTURE_PIXELS: u64 = 33_000_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowInfo {
    title: String,
    app: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureResult {
    path: String,
    relative_path: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

pub fn list_windows() -> Result<String, String> {
    let wins = xcap::Window::all().map_err(|e| format!("枚举窗口失败：{e}"))?;
    let mut out = Vec::new();
    for w in wins {
        if let Some(info) = read_window(&w) {
            out.push(info);
        }
    }
    serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
}

pub fn capture_region(state: &crate::AppState, args: Value) -> Result<String, String> {
    let x = required_i32(&args, "x")?;
    let y = required_i32(&args, "y")?;
    let width = required_u32(&args, "width")?;
    let height = required_u32(&args, "height")?;
    validate_capture_size(width, height)?;

    let img = capture_screen_region(x, y, width, height)?;
    save_capture(
        state,
        img,
        x,
        y,
        width,
        height,
        optional_label(&args).unwrap_or("region"),
    )
}

pub fn capture_window(state: &crate::AppState, args: Value) -> Result<String, String> {
    let title = super::args::optional_str(&args, "title")
        .unwrap_or("")
        .trim();
    let app = super::args::optional_str(&args, "app").unwrap_or("").trim();
    if title.is_empty() && app.is_empty() {
        return Err("title 和 app 至少提供一个，用于匹配窗口".to_string());
    }

    let (wx, wy, ww, wh) =
        find_window(title, app).ok_or("目标窗口未找到（可能已关闭、最小化或标题变化）")?;
    let l = optional_f64(&args, "crop_left", 0.0).clamp(0.0, 1.0);
    let t = optional_f64(&args, "crop_top", 0.0).clamp(0.0, 1.0);
    let r = optional_f64(&args, "crop_right", 1.0)
        .clamp(0.0, 1.0)
        .max(l + 0.02);
    let b = optional_f64(&args, "crop_bottom", 1.0)
        .clamp(0.0, 1.0)
        .max(t + 0.02);

    let x = wx + (ww as f64 * l).round() as i32;
    let y = wy + (wh as f64 * t).round() as i32;
    let width = (ww as f64 * (r - l)).round() as u32;
    let height = (wh as f64 * (b - t)).round() as u32;
    validate_capture_size(width, height)?;

    let img = capture_screen_region(x, y, width, height)?;
    save_capture(
        state,
        img,
        x,
        y,
        width,
        height,
        optional_label(&args).unwrap_or("window"),
    )
}

pub fn preview_region(args: Value) -> Result<String, String> {
    let x = required_i32(&args, "x")?;
    let y = required_i32(&args, "y")?;
    let width = required_u32(&args, "width")?;
    let height = required_u32(&args, "height")?;
    validate_capture_size(width, height)?;
    Ok(format!(
        "将读取屏幕区域 x={x}, y={y}, width={width}, height={height}，并保存到沙盒 .demiurge/screenshots/。"
    ))
}

pub fn preview_window(args: Value) -> Result<String, String> {
    let title = super::args::optional_str(&args, "title")
        .unwrap_or("")
        .trim();
    let app = super::args::optional_str(&args, "app").unwrap_or("").trim();
    if title.is_empty() && app.is_empty() {
        return Err("title 和 app 至少提供一个，用于匹配窗口".to_string());
    }
    Ok(format!(
        "将读取匹配窗口的屏幕画面（title=\"{title}\", app=\"{app}\"），并保存到沙盒 .demiurge/screenshots/。"
    ))
}

fn read_window(w: &xcap::Window) -> Option<WindowInfo> {
    if w.is_minimized().unwrap_or(false) {
        return None;
    }
    let title = w.title().unwrap_or_default();
    let app = w.app_name().unwrap_or_default();
    if title.trim().is_empty() && app.trim().is_empty() {
        return None;
    }
    let width = w.width().unwrap_or(0);
    let height = w.height().unwrap_or(0);
    if width < 80 || height < 60 {
        return None;
    }
    Some(WindowInfo {
        title,
        app,
        x: w.x().unwrap_or(0),
        y: w.y().unwrap_or(0),
        width,
        height,
    })
}

fn find_window(title: &str, app: &str) -> Option<(i32, i32, u32, u32)> {
    let wins = xcap::Window::all().ok()?;
    let mut best: Option<(i32, i32, u32, u32, u64)> = None;
    for w in wins {
        if let Some(info) = read_window(&w) {
            let title_match = !title.is_empty() && info.title == title;
            let app_match = !app.is_empty() && info.app == app;
            if title_match || app_match {
                let area = info.width as u64 * info.height as u64;
                if best.map(|b| area > b.4).unwrap_or(true) {
                    best = Some((info.x, info.y, info.width, info.height, area));
                }
            }
        }
    }
    best.map(|(x, y, w, h, _)| (x, y, w, h))
}

fn capture_screen_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, String> {
    let center_x = x.saturating_add((width / 2) as i32);
    let center_y = y.saturating_add((height / 2) as i32);
    let monitor = match xcap::Monitor::from_point(center_x, center_y) {
        Ok(monitor) => monitor,
        Err(_) => xcap::Monitor::all()
            .map_err(|e| format!("枚举显示器失败：{e}"))?
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .ok_or_else(|| "未找到显示器".to_string())?,
    };

    let mx = monitor
        .x()
        .map_err(|e| format!("读取显示器坐标失败：{e}"))?;
    let my = monitor
        .y()
        .map_err(|e| format!("读取显示器坐标失败：{e}"))?;
    let mw = monitor
        .width()
        .map_err(|e| format!("读取显示器尺寸失败：{e}"))?;
    let mh = monitor
        .height()
        .map_err(|e| format!("读取显示器尺寸失败：{e}"))?;
    let local_x = x - mx;
    let local_y = y - my;
    if local_x < 0
        || local_y < 0
        || local_x as u64 + width as u64 > mw as u64
        || local_y as u64 + height as u64 > mh as u64
    {
        return Err(format!(
            "截图区域跨越显示器边界或超出屏幕：区域=({}, {}, {}x{})，显示器=({}, {}, {}x{})",
            x, y, width, height, mx, my, mw, mh
        ));
    }

    monitor
        .capture_region(local_x as u32, local_y as u32, width, height)
        .map_err(|e| format!("截屏失败：{e}"))
}

fn save_capture(
    state: &crate::AppState,
    img: image::RgbaImage,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    label: &str,
) -> Result<String, String> {
    let sandbox = state.sandbox_dir.lock().unwrap().clone();
    let relative_dir = Path::new(".demiurge").join("screenshots");
    let out_dir = sandbox.join(&relative_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建截图目录失败：{e}"))?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let filename = format!("{}_{}.png", sanitize_label(label), ts);
    let relative_path = relative_dir.join(filename);
    let path = sandbox.join(&relative_path);

    image::DynamicImage::ImageRgba8(img)
        .save(&path)
        .map_err(|e| format!("保存截图失败：{e}"))?;

    let result = CaptureResult {
        path: path.display().to_string(),
        relative_path: relative_path_to_string(relative_path),
        x,
        y,
        width,
        height,
    };
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "capture": result,
        "note": "截图已保存到沙盒。当前工具不返回图像内容；后续 OCR/视觉模型可读取该文件。"
    }))
    .map_err(|e| e.to_string())
}

fn validate_capture_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("截图区域无效：width/height 必须大于 0".to_string());
    }
    if width > MAX_CAPTURE_SIDE || height > MAX_CAPTURE_SIDE {
        return Err(format!("截图区域过大：单边不能超过 {MAX_CAPTURE_SIDE}px"));
    }
    let pixels = width as u64 * height as u64;
    if pixels > MAX_CAPTURE_PIXELS {
        return Err(format!(
            "截图区域过大：最多 {MAX_CAPTURE_PIXELS} 像素，当前 {pixels} 像素"
        ));
    }
    Ok(())
}

fn required_i32(args: &Value, key: &str) -> Result<i32, String> {
    let n = args
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("缺少整数参数 {key}"))?;
    i32::try_from(n).map_err(|_| format!("{key} 超出 i32 范围"))
}

fn required_u32(args: &Value, key: &str) -> Result<u32, String> {
    let n = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("缺少正整数参数 {key}"))?;
    u32::try_from(n).map_err(|_| format!("{key} 超出 u32 范围"))
}

fn optional_f64(args: &Value, key: &str, default: f64) -> f64 {
    args.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn optional_label(args: &Value) -> Option<&str> {
    super::args::optional_str(args, "label")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn sanitize_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    if cleaned.trim_matches('_').is_empty() {
        "capture".to_string()
    } else {
        cleaned
    }
}

fn relative_path_to_string(path: PathBuf) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_zero_sized_capture() {
        assert!(validate_capture_size(0, 100).is_err());
        assert!(validate_capture_size(100, 0).is_err());
    }

    #[test]
    fn sanitizes_labels_for_filenames() {
        assert_eq!(sanitize_label("chat window"), "chat_window");
        assert_eq!(sanitize_label("???"), "capture");
    }

    #[test]
    fn previews_region_without_touching_screen() {
        let preview = preview_region(json!({ "x": 1, "y": 2, "width": 300, "height": 200 }))
            .expect("preview should be valid");
        assert!(preview.contains("width=300"));
    }
}

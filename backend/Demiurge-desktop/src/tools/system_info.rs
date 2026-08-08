//! system_info：基础系统状态。无外部依赖，UTC 时间由 epoch 手动换算。
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run() -> Result<String, String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let (y, mo, d, h, mi, s) = civil_from_epoch(secs);

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "未知".to_string());

    Ok(format!(
        "当前时间(UTC)：{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}\n\
         操作系统：{os}\n\
         架构：{arch}\n\
         工作目录：{cwd}\n\
         （注：时间为 UTC，本地时间请按时区换算）"
    ))
}

/// 由 UNIX 秒换算出 UTC 年月日时分秒（Howard Hinnant 的 civil_from_days 算法）。
fn civil_from_epoch(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let h = (rem / 3600) as u32;
    let mi = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;

    // days 自 1970-01-01 起
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let mo = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let y = if mo <= 2 { y + 1 } else { y };

    (y, mo, d, h, mi, s)
}

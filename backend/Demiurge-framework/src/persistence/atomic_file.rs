use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static ATOMIC_WRITE_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn backup_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".bak");
    PathBuf::from(value)
}

pub fn atomic_write(path: &Path, contents: &[u8], keep_backup: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;

    if keep_backup && path.is_file() {
        let previous = fs::read(path)
            .map_err(|e| format!("Failed to read {} before replacement: {e}", path.display()))?;
        atomic_replace_bytes(&backup_path(path), &previous)?;
    }

    atomic_replace_bytes(path, contents)
}

pub fn atomic_write_text(path: &Path, contents: &str, keep_backup: bool) -> Result<(), String> {
    atomic_write(path, contents.as_bytes(), keep_backup)
}

pub(super) fn atomic_replace_bytes(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data");
    let temp_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        ATOMIC_WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|e| format!("Failed to create {}: {e}", temp_path.display()))?;
        file.write_all(contents)
            .map_err(|e| format!("Failed to write {}: {e}", temp_path.display()))?;
        file.sync_all()
            .map_err(|e| format!("Failed to flush {}: {e}", temp_path.display()))?;
        drop(file);
        replace_file(&temp_path, path)
            .map_err(|e| format!("Failed to replace {}: {e}", path.display()))?;
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, target: *const u16, flags: u32) -> i32;
    }

    let from = from
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(not(any(unix, windows)))]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)
}

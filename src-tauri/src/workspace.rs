//! 项目工作区浏览与 Git 状态。
//!
//! 所有来自前端的路径都按“相对于当前项目根目录”解释，并在访问前做词法与
//! canonicalize 双重校验。这样既能拦截 `..` / 绝对路径，也不会让目录符号链接
//! 逃出当前项目。

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::AppState;

const MAX_PREVIEW_BYTES: usize = 256 * 1024;
const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".next",
    ".nuxt",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    "vendor",
];

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceState {
    pub path: String,
    pub name: String,
    pub is_git: bool,
    pub branch: Option<String>,
    pub dirty: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceEntryKind {
    File,
    Directory,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceEntry {
    pub name: String,
    /// 相对于项目根的 `/` 分隔路径。
    pub path: String,
    pub kind: WorkspaceEntryKind,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceFilePreview {
    /// 相对于项目根的 `/` 分隔路径。
    pub path: String,
    pub name: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
    pub binary: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct GitBranch {
    pub name: String,
    pub current: bool,
    pub remote: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GitChangedFile {
    pub path: String,
    /// Git 的精简状态码，例如 `M`、`A`、`D`、`R`、`??` 或 `AM`。
    pub status: String,
    pub staged: bool,
    pub working_tree: bool,
}

/// 选择并绑定当前会话的项目文件夹。
#[tauri::command]
pub fn select_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<WorkspaceState, String> {
    let requested = path.trim();
    if requested.is_empty() {
        return Err("请选择一个项目文件夹".to_string());
    }

    let next = canonicalize_directory(Path::new(requested))?;
    // 提前尝试枚举一次，避免接受一个当前进程实际无权读取的目录。
    fs::read_dir(&next).map_err(|e| format!("无法读取项目文件夹“{}”：{e}", next.display()))?;

    let current = state.sandbox_dir.lock().unwrap().clone();
    if !same_workspace(&current, &next) && state.busy.load(Ordering::Acquire) {
        return Err("正在生成回复，暂时不能切换项目文件夹，请先停止当前对话".to_string());
    }

    let stored_path = path_for_json(&next);
    {
        let mut sessions = state.sessions.lock().unwrap();
        let active = sessions.active.clone();
        let session = sessions
            .get_mut(&active)
            .ok_or_else(|| "当前会话不存在，无法绑定项目文件夹".to_string())?;
        session.workspace_path = stored_path;
        session.updated_at = crate::store::now_millis();
    }
    *state.sandbox_dir.lock().unwrap() = next.clone();
    state.persist_sessions();

    let snapshot = inspect_workspace(&next);
    let _ = app.emit("workspace-updated", snapshot.clone());
    Ok(snapshot)
}

/// 懒加载目录：每次只列出指定目录的直接子项。
#[tauri::command]
pub fn list_workspace_directory(
    state: State<'_, AppState>,
    relative_path: Option<String>,
) -> Result<Vec<WorkspaceEntry>, String> {
    let root = state.sandbox_dir.lock().unwrap().clone();
    list_directory_at(&root, relative_path.as_deref())
}

#[tauri::command]
pub fn read_workspace_file(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<WorkspaceFilePreview, String> {
    let root = state.sandbox_dir.lock().unwrap().clone();
    preview_file_at(&root, &relative_path)
}

#[tauri::command]
pub fn git_branches(
    state: State<'_, AppState>,
    expected_workspace_path: String,
) -> Result<Vec<GitBranch>, String> {
    let current = state.sandbox_dir.lock().unwrap();
    let root = ensure_expected_workspace(&current, &expected_workspace_path)?;
    list_git_branches_at(&root)
}

#[tauri::command]
pub fn switch_git_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    branch: String,
    expected_workspace_path: String,
) -> Result<WorkspaceState, String> {
    if state.busy.load(Ordering::Acquire) {
        return Err("正在生成回复，暂时不能切换 Git 分支，请先停止当前对话".to_string());
    }

    let branch = branch.trim();
    if branch.is_empty() {
        return Err("Git 分支名不能为空".to_string());
    }

    // Keep the workspace lock for the whole operation so another session/project switch
    // cannot invalidate the precondition between validation and `git switch`.
    let current = state.sandbox_dir.lock().unwrap();
    let root = ensure_expected_workspace(&current, &expected_workspace_path)?;
    let branches = list_git_branches_at(&root)?;
    let selected = branches
        .iter()
        .find(|item| !item.remote && item.name == branch)
        .or_else(|| branches.iter().find(|item| item.name == branch))
        .ok_or_else(|| format!("Git 分支不存在：{branch}"))?;

    if selected.current {
        return Ok(inspect_workspace(&root));
    }

    let output = if selected.remote {
        let local_name = selected
            .name
            .split_once('/')
            .map(|(_, local)| local)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("无法从远程分支名推导本地分支：{}", selected.name))?;
        if branches
            .iter()
            .any(|item| !item.remote && item.name == local_name)
        {
            run_git_command(&root, &["switch", "--", local_name])?
        } else {
            run_git_command(&root, &["switch", "--track", "--", selected.name.as_str()])?
        }
    } else {
        run_git_command(&root, &["switch", "--", selected.name.as_str()])?
    };

    ensure_git_success(output, "切换 Git 分支失败")?;
    let snapshot = inspect_workspace(&root);
    drop(current);
    let _ = app.emit("workspace-updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub fn git_changed_files(state: State<'_, AppState>) -> Result<Vec<GitChangedFile>, String> {
    let root = state.sandbox_dir.lock().unwrap().clone();
    changed_git_files_at(&root)
}

#[tauri::command]
pub fn workspace_state(state: State<'_, AppState>) -> WorkspaceState {
    let root = state.sandbox_dir.lock().unwrap().clone();
    inspect_workspace(&root)
}

/// 会话切换后恢复其项目目录。旧会话没有 `workspace_path`，或者原目录已被移动/
/// 删除时，安全回退到应用自带的 sandbox，并把归一化后的路径写回会话。
pub(crate) fn sync_active_session_workspace(state: &AppState) -> Result<(), String> {
    let configured = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .get(&sessions.active)
            .map(|session| session.workspace_path.clone())
            .unwrap_or_default()
    };
    let resolved = resolve_session_workspace(state, &configured)?;
    let stored_path = path_for_json(&resolved);

    {
        let mut sessions = state.sessions.lock().unwrap();
        let active = sessions.active.clone();
        if let Some(session) = sessions.get_mut(&active) {
            session.workspace_path = stored_path;
        }
    }
    *state.sandbox_dir.lock().unwrap() = resolved;
    Ok(())
}

/// 在真正改变活动会话前执行，防止生成过程中通过会话切换间接改变工具沙盒。
pub(crate) fn ensure_session_workspace_switch_allowed(
    state: &AppState,
    configured: &str,
) -> Result<(), String> {
    if !state.busy.load(Ordering::Acquire) {
        return Ok(());
    }
    let target = resolve_session_workspace(state, configured)?;
    let current = state.sandbox_dir.lock().unwrap().clone();
    if same_workspace(&current, &target) {
        Ok(())
    } else {
        Err("正在生成回复，暂时不能切换到使用其他项目的会话".to_string())
    }
}

fn resolve_session_workspace(state: &AppState, configured: &str) -> Result<PathBuf, String> {
    if !configured.trim().is_empty() {
        if let Ok(path) = canonicalize_directory(Path::new(configured.trim())) {
            return Ok(path);
        }
    }

    let fallback = state.data_dir.lock().unwrap().join("sandbox");
    if !fallback.exists() {
        fs::create_dir_all(&fallback)
            .map_err(|e| format!("无法创建默认项目文件夹“{}”：{e}", fallback.display()))?;
    }
    canonicalize_directory(&fallback)
}

fn list_directory_at(
    root: &Path,
    relative_path: Option<&str>,
) -> Result<Vec<WorkspaceEntry>, String> {
    let root = canonicalize_directory(root)?;
    let relative = checked_relative_path(relative_path.unwrap_or_default())?;
    let directory = resolve_existing_path(&root, &relative)?;
    if !directory.is_dir() {
        return Err(format!("不是文件夹：{}", relative_path_for_json(&relative)));
    }

    let mut entries = Vec::new();
    let iterator = fs::read_dir(&directory)
        .map_err(|e| format!("无法读取文件夹“{}”：{e}", directory.display()))?;
    for item in iterator {
        let item = item.map_err(|e| format!("读取文件夹项目失败：{e}"))?;
        let name = item.file_name().to_string_lossy().to_string();
        let item_path = item.path();

        // metadata 会跟随符号链接；随后 canonicalize containment 检查会排除指向项目外的链接。
        let Ok(metadata) = fs::metadata(&item_path) else {
            continue;
        };
        if metadata.is_dir() && is_ignored_directory(OsStr::new(&name)) {
            continue;
        }
        if !metadata.is_dir() && !metadata.is_file() {
            continue;
        }
        let Ok(canonical_item) = canonicalize_existing(&item_path) else {
            continue;
        };
        if !canonical_item.starts_with(&root) {
            continue;
        }

        let item_relative = relative.join(&name);
        entries.push(WorkspaceEntry {
            name,
            path: relative_path_for_json(&item_relative),
            kind: if metadata.is_dir() {
                WorkspaceEntryKind::Directory
            } else {
                WorkspaceEntryKind::File
            },
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }

    entries.sort_by(|a, b| {
        entry_kind_rank(&a.kind)
            .cmp(&entry_kind_rank(&b.kind))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

fn preview_file_at(root: &Path, relative_path: &str) -> Result<WorkspaceFilePreview, String> {
    let root = canonicalize_directory(root)?;
    let relative = checked_relative_path(relative_path)?;
    if relative.as_os_str().is_empty() {
        return Err("请选择要预览的文件".to_string());
    }
    let file_path = resolve_existing_path(&root, &relative)?;
    let metadata = fs::metadata(&file_path)
        .map_err(|e| format!("无法读取文件信息“{}”：{e}", file_path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "不是普通文件：{}",
            relative_path_for_json(&relative)
        ));
    }

    let mut bytes = Vec::with_capacity((metadata.len() as usize).min(MAX_PREVIEW_BYTES + 4));
    File::open(&file_path)
        .map_err(|e| format!("无法打开文件“{}”：{e}", file_path.display()))?
        .take((MAX_PREVIEW_BYTES + 4) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("无法读取文件“{}”：{e}", file_path.display()))?;

    let truncated = metadata.len() > MAX_PREVIEW_BYTES as u64;
    let sample_len = bytes.len().min(MAX_PREVIEW_BYTES);
    let sample = &bytes[..sample_len];
    let (binary, content) = decode_text_preview(sample, truncated);
    let name = relative
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(relative_path)
        .to_string();

    Ok(WorkspaceFilePreview {
        path: relative_path_for_json(&relative),
        name,
        content,
        size: metadata.len(),
        truncated,
        binary,
    })
}

fn decode_text_preview(bytes: &[u8], truncated: bool) -> (bool, String) {
    if bytes.iter().any(|byte| *byte == 0) || looks_binary(bytes) {
        return (true, String::new());
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => (false, text.to_string()),
        Err(error) if truncated && error.error_len().is_none() => {
            // 截断点落在 UTF-8 多字节字符中间；保留最后一个完整字符，不误判为二进制。
            let prefix = &bytes[..error.valid_up_to()];
            (false, String::from_utf8_lossy(prefix).to_string())
        }
        Err(_) => (true, String::new()),
    }
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let suspicious = bytes
        .iter()
        .filter(|byte| matches!(**byte, 1..=8 | 11 | 12 | 14..=31 | 127))
        .count();
    suspicious * 100 > bytes.len() * 10
}

fn list_git_branches_at(root: &Path) -> Result<Vec<GitBranch>, String> {
    ensure_git_repository(root)?;
    let current = git_stdout_optional(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let output = run_git_command(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)\t%(refname:short)\t%(upstream:short)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let output = ensure_git_success(output, "读取 Git 分支失败")?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut branches = Vec::new();
    for line in text.lines() {
        let mut columns = line.splitn(3, '\t');
        let ref_name = columns.next().unwrap_or_default();
        let name = columns.next().unwrap_or_default().trim();
        let upstream = columns.next().unwrap_or_default().trim();
        if name.is_empty() {
            continue;
        }
        let remote = ref_name.starts_with("refs/remotes/");
        if remote && ref_name.ends_with("/HEAD") {
            continue;
        }
        branches.push(GitBranch {
            name: name.to_string(),
            current: !remote && current.as_deref() == Some(name),
            remote,
            upstream: (!upstream.is_empty()).then(|| upstream.to_string()),
        });
    }
    branches.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| a.remote.cmp(&b.remote))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(branches)
}

fn changed_git_files_at(root: &Path) -> Result<Vec<GitChangedFile>, String> {
    ensure_git_repository(root)?;
    let output = run_git_command(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let output = ensure_git_success(output, "读取 Git 文件状态失败")?;
    let records: Vec<&[u8]> = output.stdout.split(|byte| *byte == 0).collect();
    let mut files = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        if record.len() < 3 {
            index += 1;
            continue;
        }
        let x = record[0] as char;
        let y = record[1] as char;
        let path_start = if record.get(2) == Some(&b' ') { 3 } else { 2 };
        let path = String::from_utf8_lossy(&record[path_start..]).replace('\\', "/");
        files.push(GitChangedFile {
            path,
            status: concise_git_status(x, y),
            staged: x != ' ' && x != '?',
            working_tree: y != ' ' || (x == '?' && y == '?'),
        });

        // porcelain -z 的 rename/copy 记录紧跟一个原路径；UI 只需要新路径。
        index += if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            2
        } else {
            1
        };
    }
    files.sort_by(|a, b| {
        a.path
            .to_lowercase()
            .cmp(&b.path.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(files)
}

fn concise_git_status(x: char, y: char) -> String {
    if x == '?' && y == '?' {
        return "??".to_string();
    }
    match (x, y) {
        (' ', code) | (code, ' ') => code.to_string(),
        (left, right) if left == right => left.to_string(),
        (left, right) => format!("{left}{right}"),
    }
}

pub(crate) fn inspect_workspace(root: &Path) -> WorkspaceState {
    let path = canonicalize_directory(root).unwrap_or_else(|_| root.to_path_buf());
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path_for_json(&path));
    let is_git = ensure_git_repository(&path).is_ok();
    let branch = if is_git {
        git_stdout_optional(&path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
    } else {
        None
    };
    let dirty = if is_git {
        git_stdout_optional(
            &path,
            &["status", "--porcelain=v1", "--untracked-files=normal"],
        )
        .is_some_and(|value| !value.is_empty())
    } else {
        false
    };
    WorkspaceState {
        path: path_for_json(&path),
        name,
        is_git,
        branch,
        dirty,
    }
}

fn ensure_git_repository(root: &Path) -> Result<(), String> {
    let output = run_git_command(root, &["rev-parse", "--is-inside-work-tree"])?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "true" {
        return Err("当前项目不是 Git 仓库".to_string());
    }
    Ok(())
}

fn git_stdout_optional(root: &Path, args: &[&str]) -> Option<String> {
    let output = run_git_command(root, args).ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git_command(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "无法运行 Git，请确认 Git 已安装并已加入 PATH".to_string()
            } else {
                format!("无法在项目目录运行 Git：{e}")
            }
        })
}

fn ensure_git_success(output: Output, context: &str) -> Result<Output, String> {
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("Git 退出码 {}", output.status)
    };
    Err(format!("{context}：{detail}"))
}

fn checked_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.contains('\0') {
        return Err("路径包含无效字符".to_string());
    }
    let path = Path::new(value.trim());
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                if is_ignored_directory(part) {
                    return Err(format!(
                        "目录“{}”已被项目浏览器忽略",
                        part.to_string_lossy()
                    ));
                }
                relative.push(part);
            }
            Component::ParentDir => {
                return Err("路径不能包含“..”，只能访问当前项目内的文件".to_string())
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("请使用相对于当前项目根目录的路径".to_string())
            }
        }
    }
    Ok(relative)
}

fn resolve_existing_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let root = canonicalize_directory(root)?;
    let target = canonicalize_existing(&root.join(relative)).map_err(|e| {
        format!(
            "项目路径不存在或无法访问“{}”：{e}",
            relative_path_for_json(relative)
        )
    })?;
    if !target.starts_with(&root) {
        return Err("拒绝访问项目根目录之外的路径".to_string());
    }
    Ok(target)
}

fn canonicalize_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("项目文件夹不存在：{}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("所选路径不是文件夹：{}", path.display()));
    }
    canonicalize_existing(path).map_err(|e| format!("无法解析项目文件夹“{}”：{e}", path.display()))
}

fn canonicalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    fs::canonicalize(path).map(clean_windows_verbatim_prefix)
}

#[cfg(windows)]
fn clean_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

#[cfg(not(windows))]
fn clean_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

fn same_workspace(left: &Path, right: &Path) -> bool {
    match (canonicalize_existing(left), canonicalize_existing(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn ensure_expected_workspace(current: &Path, expected: &str) -> Result<PathBuf, String> {
    let expected = expected.trim();
    if expected.is_empty() {
        return Err("缺少分支操作对应的项目路径，请重新打开分支列表".to_string());
    }

    let current = canonicalize_directory(current)?;
    let expected = canonicalize_directory(Path::new(expected))
        .map_err(|_| "项目已切换，请重新打开分支列表后再操作".to_string())?;
    if current != expected {
        return Err("项目已切换，请重新打开分支列表后再操作".to_string());
    }
    Ok(current)
}

fn is_ignored_directory(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    IGNORED_DIRECTORIES
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

fn entry_kind_rank(kind: &WorkspaceEntryKind) -> u8 {
    match kind {
        WorkspaceEntryKind::Directory => 0,
        WorkspaceEntryKind::File => 1,
    }
}

fn relative_path_for_json(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn path_for_json(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn temp_directory(label: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "demiurge_workspace_{label}_{timestamp}_{}",
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn git_ok(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git must be available for workspace tests");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn initialize_git_repository(root: &Path) {
        git_ok(root, &["init", "-b", "main"]);
        git_ok(root, &["config", "user.email", "tests@demiurge.local"]);
        git_ok(root, &["config", "user.name", "Demiurge Tests"]);
        fs::write(root.join("tracked.txt"), "initial\n").unwrap();
        git_ok(root, &["add", "--", "tracked.txt"]);
        git_ok(root, &["commit", "-m", "initial"]);
        git_ok(root, &["branch", "feature/workspace"]);
    }

    #[test]
    fn relative_paths_reject_escape_and_absolute_inputs() {
        assert!(checked_relative_path("../secret.txt").is_err());
        assert!(checked_relative_path("folder/../../secret.txt").is_err());
        assert!(checked_relative_path(".git/config").is_err());

        #[cfg(windows)]
        {
            assert!(checked_relative_path(r"C:\Windows\win.ini").is_err());
            assert!(checked_relative_path(r"\\server\share\file.txt").is_err());
        }
        #[cfg(not(windows))]
        assert!(checked_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn git_workspace_precondition_accepts_current_and_rejects_stale_paths() {
        let current = temp_directory("git_precondition_current");
        let stale = temp_directory("git_precondition_stale");

        let resolved = ensure_expected_workspace(&current, &path_for_json(&current)).unwrap();
        assert!(same_workspace(&resolved, &current));

        let error = ensure_expected_workspace(&current, &path_for_json(&stale)).unwrap_err();
        assert!(error.contains("项目已切换"));
        assert!(ensure_expected_workspace(&current, "").is_err());

        fs::remove_dir_all(current).unwrap();
        fs::remove_dir_all(stale).unwrap();
    }

    #[test]
    fn directory_listing_is_lazy_and_filters_heavy_directories() {
        let root = temp_directory("listing");
        fs::create_dir_all(root.join("src").join("nested")).unwrap();
        fs::create_dir_all(root.join("node_modules").join("dependency")).unwrap();
        fs::create_dir_all(root.join(".git").join("objects")).unwrap();
        fs::write(root.join("README.md"), "hello").unwrap();

        let entries = list_directory_at(&root, None).unwrap();
        assert!(entries.iter().any(|item| item.path == "src"));
        assert!(entries.iter().any(|item| item.path == "README.md"));
        assert!(!entries.iter().any(|item| item.path == "src/nested"));
        assert!(!entries.iter().any(|item| item.name == "node_modules"));
        assert!(!entries.iter().any(|item| item.name == ".git"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_branches_and_changed_files_are_normalized_for_the_ui() {
        let root = temp_directory("git_state");
        initialize_git_repository(&root);

        let branches = list_git_branches_at(&root).unwrap();
        assert!(branches
            .iter()
            .any(|branch| branch.name == "main" && branch.current && !branch.remote));
        assert!(branches
            .iter()
            .any(|branch| branch.name == "feature/workspace" && !branch.current));

        fs::write(root.join("tracked.txt"), "changed\n").unwrap();
        fs::write(root.join("new file.txt"), "new\n").unwrap();
        let changes = changed_git_files_at(&root).unwrap();
        assert!(changes
            .iter()
            .any(|file| { file.path == "tracked.txt" && file.status == "M" && file.working_tree }));
        assert!(changes
            .iter()
            .any(|file| file.path == "new file.txt" && file.status == "??"));

        git_ok(&root, &["restore", "--", "tracked.txt"]);
        fs::remove_file(root.join("new file.txt")).unwrap();
        git_ok(&root, &["switch", "--", "feature/workspace"]);
        let switched = list_git_branches_at(&root).unwrap();
        assert!(switched
            .iter()
            .any(|branch| branch.name == "feature/workspace" && branch.current));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_truncates_text_without_loading_the_whole_file_and_marks_binary() {
        let root = temp_directory("preview");
        let text = "文".repeat(MAX_PREVIEW_BYTES);
        fs::write(root.join("large.txt"), text.as_bytes()).unwrap();
        fs::write(root.join("image.bin"), [0_u8, 1, 2, 3]).unwrap();

        let preview = preview_file_at(&root, "large.txt").unwrap();
        assert!(preview.truncated);
        assert!(!preview.binary);
        assert!(preview.content.len() <= MAX_PREVIEW_BYTES);

        let binary = preview_file_at(&root, "image.bin").unwrap();
        assert!(binary.binary);
        assert!(binary.content.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_cannot_escape_workspace_root() {
        use std::os::unix::fs::symlink;

        let root = temp_directory("symlink_root");
        let outside = temp_directory("symlink_outside");
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(&outside, root.join("outside-link")).unwrap();

        let error = preview_file_at(&root, "outside-link/secret.txt").unwrap_err();
        assert!(error.contains("项目根目录之外"));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn symlink_cannot_escape_workspace_root_when_supported() {
        use std::os::windows::fs::symlink_dir;

        let root = temp_directory("symlink_root");
        let outside = temp_directory("symlink_outside");
        fs::write(outside.join("secret.txt"), "secret").unwrap();

        // Windows 未开启开发者模式且进程没有管理员权限时无法创建符号链接；该环境下
        // 词法路径用例仍然执行，本用例只跳过 OS 不允许构造的部分。
        if symlink_dir(&outside, root.join("outside-link")).is_ok() {
            let error = preview_file_at(&root, "outside-link/secret.txt").unwrap_err();
            assert!(error.contains("项目根目录之外"));
        }

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .expect("architecture directory must exist")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("rs"))
        .collect()
}

#[test]
fn tauri_commands_live_only_in_controller_adapters() {
    let src = manifest_dir().join("src");
    let controller = src.join("controller");

    for path in rust_files(&controller) {
        if path.file_name().and_then(|value| value.to_str()) == Some("mod.rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("controller source must be readable");
        assert!(
            source.contains("#[tauri::command]"),
            "{} must expose at least one IPC adapter",
            path.display()
        );
    }

    for path in rust_files(&src) {
        let source = fs::read_to_string(&path).expect("desktop source must be readable");
        assert!(
            !source
                .lines()
                .any(|line| line.trim() == "#[tauri::command]"),
            "{} must not define a Tauri command outside controller/",
            path.display()
        );
    }
}

#[test]
fn controllers_do_not_reach_state_storage_or_external_sdks() {
    let controller = manifest_dir().join("src").join("controller");
    let forbidden = [
        ".sessions.lock(",
        ".settings.lock(",
        ".http.",
        "persist_sessions(",
        "persist_settings(",
        "reqwest::",
        "std::fs::",
        "tokio::process",
    ];

    for path in rust_files(&controller) {
        let source = fs::read_to_string(&path).expect("controller source must be readable");
        for token in forbidden {
            assert!(
                !source.contains(token),
                "{} bypasses Biz/Service boundary with `{token}`",
                path.display()
            );
        }
    }
}

#[test]
fn starter_and_library_root_only_assemble_runtime() {
    let root = fs::read_to_string(manifest_dir().join("src").join("lib.rs"))
        .expect("lib.rs must be readable");
    assert!(
        root.lines().count() <= 40,
        "lib.rs must remain a thin facade"
    );
    assert!(!root.contains("#[tauri::command]"));

    let starter = fs::read_to_string(manifest_dir().join("src").join("starter.rs"))
        .expect("starter.rs must be readable");
    for token in [
        "run_turn_with_options(",
        "execute_tool(",
        "memory_add_entry(",
        "webdav_backup_now(",
    ] {
        assert!(
            !starter.contains(token),
            "starter must not contain domain behavior: `{token}`"
        );
    }
}

#[test]
fn cargo_crates_follow_the_dependency_direction() {
    let desktop = manifest_dir();
    let backend = desktop
        .parent()
        .expect("desktop crate must live under backend");
    let common = fs::read_to_string(backend.join("Demiurge-common").join("Cargo.toml"))
        .expect("common manifest");
    let core = fs::read_to_string(backend.join("Demiurge-core").join("Cargo.toml"))
        .expect("core manifest");
    let framework = fs::read_to_string(backend.join("Demiurge-framework").join("Cargo.toml"))
        .expect("framework manifest");
    let desktop = fs::read_to_string(desktop.join("Cargo.toml")).expect("desktop manifest");

    assert!(!common.contains("demiurge-core"));
    assert!(!common.contains("demiurge-framework"));
    assert!(!common.contains("demiurge ="));
    assert!(core.contains("demiurge-common"));
    assert!(!core.contains("demiurge-framework"));
    assert!(!core.contains("demiurge ="));
    assert!(framework.contains("demiurge-common"));
    assert!(framework.contains("demiurge-core"));
    assert!(!framework.contains("demiurge ="));
    assert!(desktop.contains("demiurge-common"));
    assert!(desktop.contains("demiurge-core"));
    assert!(desktop.contains("demiurge-framework"));
}

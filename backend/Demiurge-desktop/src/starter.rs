//! Tauri 运行时装配。
//!
//! Starter 只创建基础设施、恢复运行状态、注册窗口和 Controller，
//! 不承载会话、Agent、Workspace 等领域行为。

pub(crate) mod state;

use std::time::Duration;

use tauri::{Manager, PhysicalSize, Size};

// `generate_handler!` 除函数本身还要解析 `#[tauri::command]` 生成的隐藏宏。
// 在命令全部迁入 Controller 模块前，这个 glob 只允许存在于 Starter 注册表。
use crate::controller::agent::*;
use crate::controller::agent_config::*;
use crate::controller::companion::*;
use crate::controller::goal::*;
use crate::controller::mcp::*;
use crate::controller::media::*;
use crate::controller::memory::*;
use crate::controller::pack::*;
use crate::controller::permission::*;
use crate::controller::remote::*;
use crate::controller::session::*;
use crate::controller::settings::*;
use crate::controller::system::*;
use crate::controller::voice::*;
use crate::controller::window::*;
use crate::controller::workflow::*;
use crate::controller::workflow_entry::*;
use crate::controller::workspace::*;
use crate::*;

pub(crate) fn run() {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest::Client");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(http))
        .setup(setup_runtime)
        .invoke_handler(tauri::generate_handler![
            send,
            send_with_agents,
            interrupt,
            session_engine_state,
            respond_confirm,
            get_settings,
            save_settings,
            provider_check_connection,
            web_search_check_connection,
            set_permission_mode,
            plan_state,
            approve_plan,
            reject_plan,
            webdav_check_connection,
            webdav_backup_now,
            webdav_list_backups,
            webdav_delete_backup,
            permission_panel_state,
            shell_policy_state,
            permission_reset_rule,
            permission_upsert_rule,
            mcp_panel_state,
            mcp_refresh,
            mcp_set_server_enabled,
            list_packs,
            import_pack_zip,
            open_pack_dir,
            import_pack_lore_files,
            list_pack_files,
            read_pack_file,
            lorebook_index_status,
            lorebook_recall_detail,
            lorebook_rebuild_index,
            embedding_probe,
            read_pack_manifest_json,
            save_pack_manifest_json,
            preview_pack_lorebook,
            import_pack_live2d_folder,
            resolve_pack_live2d_path,
            pack_live2d_bundle,
            remove_pack_live2d,
            agent_panel_state,
            agent_template_json,
            agent_validate_json,
            agent_read_file,
            agent_save_file,
            agent_delete_file,
            goal_panel_state,
            goal_pause,
            goal_resume,
            goal_continue,
            goal_clear,
            memory_panel_state,
            memory_add_entry,
            memory_update_entry,
            memory_delete_entry,
            memory_dedupe_apply,
            memory_migrate_namespace,
            list_sessions,
            navigation_snapshot,
            session_stats,
            get_history,
            context_panel_state,
            skill_panel_state,
            open_skills_dir,
            new_session,
            select_session,
            delete_session,
            rename_session,
            open_sandbox,
            workspace_state,
            select_workspace,
            list_workspace_directory,
            read_workspace_file,
            git_branches,
            switch_git_branch,
            git_changed_files,
            ocr_image_bytes,
            media_generate_image,
            media_synthesize_speech,
            companion_panel_state,
            companion_clear_weather_cache,
            main_window_minimize,
            main_window_toggle_maximize,
            main_window_close,
            desktop_companion_restore,
            desktop_companion_show_main,
            open_widgets_window,
            open_live2d_window,
            pomodoro_state,
            pomodoro_start,
            pomodoro_pause,
            pomodoro_resume,
            pomodoro_skip,
            companion_memory_suggestions,
            companion_memory_queue_state,
            companion_enqueue_memory_suggestion,
            companion_save_memory_queue_item,
            companion_ignore_memory_queue_item,
            companion_save_all_memory_queue_items,
            companion_ignore_all_memory_queue_items,
            companion_undo_memory_queue_item,
            ocr_model_status,
            ocr_download_models,
            workflow_panel_state,
            workflow_run,
            workflow_stop,
            workflow_validate,
            workflow_dry_run,
            workflow_templates,
            workflow_install_template,
            workflow_run_with_inputs,
            workflow_retry_failed_node,
            voice_status,
            voice_transcribe,
            voice_synthesize,
            voice_tts_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_runtime(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(Size::Physical(PhysicalSize {
            width: crate::biz::window::DEFAULT_WINDOW_WIDTH,
            height: crate::biz::window::DEFAULT_WINDOW_HEIGHT,
        }));
        let _ = window.center();
    }

    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let sandbox = dir.join("sandbox");
    std::fs::create_dir_all(&sandbox)?;
    let packs = dir.join("packs");
    std::fs::create_dir_all(&packs)?;
    pack::ensure_default(&packs)?;

    let mut settings = store::load_settings(&dir);
    if let Err(error) = credentials::hydrate_or_migrate_settings(&dir, &mut settings) {
        eprintln!("Demiurge credential warning: {error}");
    }
    if let Err(error) = startup::apply_launch_on_startup(settings.launch_on_startup) {
        eprintln!("Demiurge startup integration warning: {error}");
    }
    let sessions = store::load_sessions(&dir);

    let state = app.state::<AppState>();
    *state.data_dir.lock().unwrap() = dir;
    *state.sandbox_dir.lock().unwrap() = sandbox;
    *state.packs_dir.lock().unwrap() = packs;
    *state.settings.lock().unwrap() = settings;
    *state.sessions.lock().unwrap() = sessions;
    if let Err(error) = workspace::sync_active_session_workspace(state.inner()) {
        eprintln!("Demiurge workspace restore warning: {error}");
    }
    let settings_snapshot = state.settings.lock().unwrap().clone();
    if let Err(error) =
        crate::biz::window::sync_desktop_companion_window(app.handle(), &settings_snapshot)
    {
        eprintln!("Demiurge desktop companion startup warning: {error}");
    }
    crate::biz::window::bind_main_window_lifecycle(app.handle());

    // Windows 上从 command 线程动态创建 WebviewWindow 只会得到空白外壳，
    // 因此启动时预创建并隐藏，打开时只做 show/focus。
    if let Err(error) = crate::biz::window::create_widgets_window(app.handle()) {
        eprintln!("Demiurge widgets window startup warning: {error}");
    }
    if let Err(error) = crate::biz::window::create_live2d_window(app.handle()) {
        eprintln!("Demiurge Live2D window startup warning: {error}");
    }
    agent::workflow_runtime::hydrate_persisted_runs(state.inner());
    pomodoro::hydrate(app.handle().clone(), state.inner());
    state.persist_sessions();
    Ok(())
}

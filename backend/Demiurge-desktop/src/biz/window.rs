//! Window lifecycle and native window orchestration.

use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size};

use crate::store::Settings;
use crate::AppState;

pub(crate) const DEFAULT_WINDOW_WIDTH: u32 = 1811;
pub(crate) const DEFAULT_WINDOW_HEIGHT: u32 = 1213;
pub(crate) const DESKTOP_COMPANION_WINDOW_LABEL: &str = "desktop_companion";
pub(crate) const DESKTOP_COMPANION_EXPANDED_WIDTH: u32 = 320;
pub(crate) const DESKTOP_COMPANION_EXPANDED_HEIGHT: u32 = 178;
pub(crate) const DESKTOP_COMPANION_COLLAPSED_WIDTH: u32 = 188;
pub(crate) const DESKTOP_COMPANION_COLLAPSED_HEIGHT: u32 = 64;
pub(crate) const WIDGETS_WINDOW_LABEL: &str = "widgets";
pub(crate) const WIDGETS_WINDOW_WIDTH: f64 = 520.0;
pub(crate) const WIDGETS_WINDOW_HEIGHT: f64 = 700.0;
pub(crate) const LIVE2D_WINDOW_LABEL: &str = "live2d";
pub(crate) const LIVE2D_VISIBILITY_EVENT: &str = "live2d-visibility-changed";
pub(crate) const LIVE2D_WINDOW_WIDTH: f64 = 430.0;
pub(crate) const LIVE2D_WINDOW_HEIGHT: f64 = 640.0;

pub(crate) fn emit_settings_updated(app: &AppHandle, settings: &Settings) {
    let _ = app.emit("settings-updated", settings.clone());
}

pub(crate) fn desktop_companion_size(settings: &Settings) -> PhysicalSize<u32> {
    if settings.desktop_companion_collapsed {
        PhysicalSize {
            width: DESKTOP_COMPANION_COLLAPSED_WIDTH,
            height: DESKTOP_COMPANION_COLLAPSED_HEIGHT,
        }
    } else {
        PhysicalSize {
            width: DESKTOP_COMPANION_EXPANDED_WIDTH,
            height: DESKTOP_COMPANION_EXPANDED_HEIGHT,
        }
    }
}

pub(crate) fn ensure_desktop_companion_window(
    app: &AppHandle,
    settings: &Settings,
) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(DESKTOP_COMPANION_WINDOW_LABEL) {
        return Ok(window);
    }

    let size = desktop_companion_size(settings);
    tauri::WebviewWindowBuilder::new(
        app,
        DESKTOP_COMPANION_WINDOW_LABEL,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Demiurge Companion")
    .inner_size(size.width as f64, size.height as f64)
    .min_inner_size(
        DESKTOP_COMPANION_COLLAPSED_WIDTH as f64,
        DESKTOP_COMPANION_COLLAPSED_HEIGHT as f64,
    )
    .max_inner_size(
        DESKTOP_COMPANION_EXPANDED_WIDTH as f64,
        DESKTOP_COMPANION_EXPANDED_HEIGHT as f64,
    )
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(settings.desktop_companion_always_on_top)
    .skip_taskbar(true)
    .focused(false)
    // This builder is only reached for an enabled companion. Creating it hidden
    // and immediately calling show() during setup can race with the initial
    // Windows visibility state, leaving the shell hidden after startup.
    .visible(true)
    .center()
    .build()
    .map_err(|e| format!("Failed to create desktop companion window: {e}"))
}

/// 构建常驻的小工具窗口。窗口始终以隐藏状态创建：Windows 上在 async command
/// 上下文动态创建 WebviewWindow 会得到只有外壳、没有渲染内容的空白窗口，
/// 因此改为启动时在主线程预创建，打开时只做显示/恢复/聚焦。
pub(crate) fn create_widgets_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let window = tauri::WebviewWindowBuilder::new(
        app,
        WIDGETS_WINDOW_LABEL,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Demiurge - Utilities")
    .inner_size(WIDGETS_WINDOW_WIDTH, WIDGETS_WINDOW_HEIGHT)
    .min_inner_size(420.0, 540.0)
    .resizable(true)
    .decorations(true)
    .visible(false)
    .center()
    .build()
    .map_err(|e| format!("Failed to create widgets window: {e}"))?;
    bind_widgets_window_lifecycle(&window);
    Ok(window)
}

/// Live2D 挂件窗口只预创建透明 webview，不在隐藏阶段加载模型。前端在窗口首次
/// 获得焦点后才挂载 Live2DPanel，避免拖慢主窗口启动；后续隐藏时保留渲染资源。
pub(crate) fn create_live2d_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let window = tauri::WebviewWindowBuilder::new(
        app,
        LIVE2D_WINDOW_LABEL,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Demiurge - Live2D")
    .inner_size(LIVE2D_WINDOW_WIDTH, LIVE2D_WINDOW_HEIGHT)
    .min_inner_size(300.0, 420.0)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .center()
    .build()
    .map_err(|e| format!("Failed to create Live2D window: {e}"))?;
    bind_live2d_window_lifecycle(&window);
    Ok(window)
}

/// 小工具窗口常驻存活：关闭按钮只隐藏窗口，webview 保持已渲染状态，
/// 下次打开可以瞬时恢复，也避免重建时再次踩到空白窗口问题。
pub(crate) fn bind_widgets_window_lifecycle(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(window) = app.get_webview_window(WIDGETS_WINDOW_LABEL) {
                let _ = window.hide();
            }
        }
    });
}

pub(crate) fn bind_live2d_window_lifecycle(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(window) = app.get_webview_window(LIVE2D_WINDOW_LABEL) {
                let _ = window.emit(LIVE2D_VISIBILITY_EVENT, false);
                let _ = window.hide();
            }
        }
    });
}

/// 主窗口是应用主体：关闭主窗口即退出进程。小工具/桌宠窗口是常驻的隐藏窗口，
/// 不主动退出会让事件循环一直持有它们，进程无法结束。
pub(crate) fn bind_main_window_lifecycle(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let app = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
            app.exit(0);
        }
    });
}

/// 显示常驻小工具窗口：先取消最小化，再显示并聚焦。
pub(crate) fn show_widgets_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    window
        .unminimize()
        .map_err(|e| format!("Failed to restore widgets window: {e}"))?;
    window
        .show()
        .map_err(|e| format!("Failed to show widgets window: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("Failed to focus widgets window: {e}"))?;
    Ok(())
}

pub(crate) fn show_live2d_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    window
        .unminimize()
        .map_err(|e| format!("Failed to restore Live2D window: {e}"))?;
    window
        .show()
        .map_err(|e| format!("Failed to show Live2D window: {e}"))?;
    window
        .emit(LIVE2D_VISIBILITY_EVENT, true)
        .map_err(|e| format!("Failed to resume Live2D rendering: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("Failed to focus Live2D window: {e}"))?;
    Ok(())
}

pub(crate) fn sync_desktop_companion_window(
    app: &AppHandle,
    settings: &Settings,
) -> Result<(), String> {
    if !settings.desktop_companion_enabled {
        if let Some(window) = app.get_webview_window(DESKTOP_COMPANION_WINDOW_LABEL) {
            let _ = window.set_ignore_cursor_events(false);
            window
                .hide()
                .map_err(|e| format!("Failed to hide desktop companion window: {e}"))?;
        }
        return Ok(());
    }

    let window = ensure_desktop_companion_window(app, settings)?;
    let size = desktop_companion_size(settings);
    window
        .set_always_on_top(settings.desktop_companion_always_on_top)
        .map_err(|e| format!("Failed to update desktop companion pin state: {e}"))?;
    window
        .set_skip_taskbar(true)
        .map_err(|e| format!("Failed to update desktop companion taskbar state: {e}"))?;
    window
        .set_size(Size::Logical(tauri::LogicalSize {
            width: size.width as f64,
            height: size.height as f64,
        }))
        .map_err(|e| format!("Failed to resize desktop companion window: {e}"))?;
    window
        .set_ignore_cursor_events(settings.desktop_companion_click_through)
        .map_err(|e| format!("Failed to update desktop companion click-through state: {e}"))?;
    window
        .show()
        .map_err(|e| format!("Failed to show desktop companion window: {e}"))?;
    Ok(())
}

pub(crate) fn main_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Main window is not available".to_string())
}

#[cfg(target_os = "windows")]
pub(crate) mod native_main_window {
    use std::ffi::c_void;

    use tauri::WebviewWindow;

    const SW_MAXIMIZE: i32 = 3;
    const SW_MINIMIZE: i32 = 6;
    const SW_RESTORE: i32 = 9;
    const WM_CLOSE: u32 = 0x0010;

    #[link(name = "user32")]
    extern "system" {
        fn IsZoomed(window: *mut c_void) -> i32;
        fn PostMessageW(window: *mut c_void, message: u32, w_param: usize, l_param: isize) -> i32;
        fn ShowWindow(window: *mut c_void, command: i32) -> i32;
    }

    fn hwnd(window: &WebviewWindow) -> Result<*mut c_void, String> {
        window
            .hwnd()
            .map(|handle| handle.0 as *mut c_void)
            .map_err(|e| format!("Failed to resolve native main window handle: {e}"))
    }

    pub fn minimize(window: &WebviewWindow) -> Result<(), String> {
        let handle = hwnd(window)?;
        // SAFETY: Tauri owns this live top-level HWND for the duration of the call.
        unsafe {
            ShowWindow(handle, SW_MINIMIZE);
        }
        Ok(())
    }

    pub fn toggle_maximize(window: &WebviewWindow) -> Result<(), String> {
        let handle = hwnd(window)?;
        // SAFETY: Tauri owns this live top-level HWND for the duration of the call.
        let command = if unsafe { IsZoomed(handle) } != 0 {
            SW_RESTORE
        } else {
            SW_MAXIMIZE
        };
        // SAFETY: The command is one of the documented ShowWindow constants.
        unsafe {
            ShowWindow(handle, command);
        }
        Ok(())
    }

    pub fn close(window: &WebviewWindow) -> Result<(), String> {
        let handle = hwnd(window)?;
        // Post WM_CLOSE instead of destroying the HWND so Tauri still emits
        // CloseRequested and the main-window lifecycle can exit cleanly.
        if unsafe { PostMessageW(handle, WM_CLOSE, 0, 0) } == 0 {
            return Err("Failed to post close request to the main window".to_string());
        }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) mod native_main_window {
    use tauri::WebviewWindow;

    pub fn minimize(window: &WebviewWindow) -> Result<(), String> {
        window
            .minimize()
            .map_err(|e| format!("Failed to minimize main window: {e}"))
    }

    pub fn toggle_maximize(window: &WebviewWindow) -> Result<(), String> {
        window
            .toggle_maximize()
            .map_err(|e| format!("Failed to toggle main window maximize state: {e}"))
    }

    pub fn close(window: &WebviewWindow) -> Result<(), String> {
        window
            .close()
            .map_err(|e| format!("Failed to close main window: {e}"))
    }
}

// IPC-independent window use cases.
pub(crate) fn main_window_minimize(app: AppHandle) -> Result<(), String> {
    native_main_window::minimize(&main_window(&app)?)
}

pub(crate) fn main_window_toggle_maximize(app: AppHandle) -> Result<(), String> {
    native_main_window::toggle_maximize(&main_window(&app)?)
}

pub(crate) fn main_window_close(app: AppHandle) -> Result<(), String> {
    native_main_window::close(&main_window(&app)?)
}

pub(crate) fn desktop_companion_restore(app: AppHandle, state: &AppState) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    if !settings.desktop_companion_enabled {
        // 设置仍是唯一权威：未启用时呼出不应凭空造出一个陪伴壳。
        return Ok(());
    }
    // 窗口已存在时这一步只是把置顶/尺寸/穿透同步一遍，是幂等的。
    sync_desktop_companion_window(&app, &settings)?;
    let window = ensure_desktop_companion_window(&app, &settings)?;
    window
        .unminimize()
        .map_err(|e| format!("Failed to restore desktop companion window: {e}"))?;
    window
        .show()
        .map_err(|e| format!("Failed to show desktop companion window: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("Failed to focus desktop companion window: {e}"))?;
    Ok(())
}

pub(crate) fn desktop_companion_show_main(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    window
        .show()
        .map_err(|e| format!("Failed to show main window: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("Failed to focus main window: {e}"))?;
    Ok(())
}

pub(crate) async fn open_widgets_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WIDGETS_WINDOW_LABEL) {
        return show_widgets_window(&window);
    }

    // 兜底：启动时的预创建失败了。Windows 上从命令线程直接建窗口只会得到空白
    // 外壳，所以把重建排回主线程，再在主线程里显示。
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = create_widgets_window(&handle).and_then(|window| show_widgets_window(&window));
        if let Err(e) = result {
            eprintln!("Demiurge widgets window warning: {e}");
        }
    })
    .map_err(|e| format!("Failed to schedule widgets window creation: {e}"))
}

pub(crate) async fn open_live2d_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LIVE2D_WINDOW_LABEL) {
        return show_live2d_window(&window);
    }

    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = create_live2d_window(&handle).and_then(|window| show_live2d_window(&window));
        if let Err(e) = result {
            eprintln!("Demiurge Live2D window warning: {e}");
        }
    })
    .map_err(|e| format!("Failed to schedule Live2D window creation: {e}"))
}

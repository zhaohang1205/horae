pub mod commands;
mod state;

use tauri::Manager;

pub use state::AppState;

/// Linux 下 WebView 渲染的已知兼容性问题兜底。
/// Hyprland/Wayland 下 webkit2gtk 默认 DMABUF 渲染器会触发
/// `Error 71 (Protocol error) dispatching to Wayland display`，关闭它即可正常显示
/// （对其他 Wayland 桌面也普遍安全）。
/// 若同时存在 X 服务（如 Hyprland 配套的 Xwayland，对应 `DISPLAY`），优先走 X11
/// 后端作为兜底，避开原生 Wayland 渲染路径的兼容问题；纯 Wayland 无 X 时不强制。
/// 必须在首个 webview 创建前（即 `tauri::Builder::run` 之前）设置，webkit 在此时读取。
#[cfg(target_os = "linux")]
fn apply_linux_webview_workarounds() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    if std::env::var_os("DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

/// 启动 Tauri 应用：在 setup 中打开默认 profile 的数据库连接并交由 State 管理，
/// 注册所有 GUI command。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    apply_linux_webview_workarounds();

    tauri::Builder::default()
        .setup(|app| {
            let conn = horae_core::db::conn::open(None)
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            app.manage(AppState(std::sync::Mutex::new(conn)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::tasks::list_tasks,
            commands::tasks::capture,
            commands::tasks::transition,
            commands::tasks::set_due,
            commands::tasks::schedule,
            commands::tasks::archive,
            commands::tasks::unarchive,
            commands::tasks::purge,
            commands::tasks::detail,
            commands::tasks::rename,
            commands::tasks::update_notes,
            commands::tasks::toggle_checklist_item,
            commands::tags::list_tags,
            commands::tags::create_tag,
            commands::tags::delete_tag,
            commands::tags::add_tag_to_task,
            commands::tags::remove_tag_from_task,
            commands::tags::get_task_tags,
            commands::pomo::pomo_state,
            commands::pomo::start_pomo,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::notifications::tick_notifications,
            commands::profiles::list_profiles,
        ])
        .run(tauri::generate_context!())
        .expect("error while running horae GUI");
}

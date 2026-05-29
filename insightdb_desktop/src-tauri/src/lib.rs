mod commands;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::execute_query,
            commands::cancel_query,
            commands::diagnose,
            commands::ai_explain,
            commands::sanitize_context,
            commands::ping,
        ])
        .run(tauri::generate_context!())
        .expect("启动 InsightDB 失败");
}

mod commands;
mod sql_guard;

use commands::{AppState, StorageState};
use insightdb_storage::ReportStorage;
use std::path::PathBuf;

fn default_storage_path() -> PathBuf {
    let dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = dir.join("insightdb");
    std::fs::create_dir_all(&app_dir).ok();
    app_dir.join("reports.db")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志：默认 info 级别，可通过 RUST_LOG 环境变量覆盖
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    log::info!("InsightDB 启动中...");

    let storage_path = default_storage_path();
    let storage = ReportStorage::open(&storage_path)
        .expect("无法打开报告数据库");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .manage(StorageState::new(storage))
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::execute_query,
            commands::cancel_query,
            commands::diagnose,
            commands::ai_explain,
            commands::sanitize_context,
            commands::ping,
            commands::save_report,
            commands::list_reports,
            commands::get_report,
            commands::delete_report,
        ])
        .run(tauri::generate_context!())
        .expect("启动 InsightDB 失败");
}

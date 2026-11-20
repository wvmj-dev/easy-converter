mod commands;
// Public so the integration tests in `tests/` can drive operations end to end.
pub mod error;
pub mod fs_utils;
pub mod jobs;
pub mod ops;
pub mod settings;
pub mod tools;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Manager, RunEvent};

use crate::fs_utils::TempSpace;
use crate::jobs::{JobQueue, TauriEventSink};
use crate::settings::SettingsState;
use crate::tools::ToolRegistry;

/// How long to wait for running jobs to stop when the app closes.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            let settings = Arc::new(SettingsState::load(&handle)?);
            let temp_base = std::env::temp_dir().join(&app.config().identifier);
            let temp = TempSpace::create(&temp_base)?;
            let config = {
                let settings = Arc::clone(&settings);
                Arc::new(move || settings.get())
            };
            let queue = JobQueue::new(temp, Arc::new(TauriEventSink::new(handle.clone())), config);

            app.manage(queue);
            app.manage(ToolRegistry::default());
            app.manage(settings);

            // Resolve tools in the background so the window opens right away.
            tauri::async_runtime::spawn(async move {
                let registry = handle.state::<ToolRegistry>();
                let settings = handle.state::<Arc<SettingsState>>();
                let ctx = commands::resolve_context(&handle, &settings.get());
                registry.statuses(&ctx).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::enqueue_job,
            commands::cancel_job,
            commands::get_tools_status,
            commands::refresh_tools,
            commands::probe_file,
            commands::get_settings,
            commands::update_settings,
            commands::open_output,
            commands::reveal_output,
        ])
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            eprintln!("failed to start application: {err}");
            std::process::exit(1);
        }
    };

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            let queue = handle.state::<JobQueue>().inner().clone();
            tauri::async_runtime::block_on(queue.shutdown(SHUTDOWN_TIMEOUT));
            queue.temp().remove_all();
        }
    });
}

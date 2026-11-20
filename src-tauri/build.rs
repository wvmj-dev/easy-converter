// Every app command gets its own `allow-<command>` permission. Once an app manifest is
// declared, a command can only be called from a window whose capability grants it, so
// adding a command never exposes it by accident.
const COMMANDS: &[&str] = &[
    "enqueue_job",
    "cancel_job",
    "get_tools_status",
    "refresh_tools",
    "probe_file",
    "get_settings",
    "update_settings",
    "open_output",
    "reveal_output",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}

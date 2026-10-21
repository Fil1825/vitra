//! Vitra desktop app: Tauri setup and command registration.

mod sidecar;

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sidecar::sidecar_status])
        .run(tauri::generate_context!())
        .expect("error while running Vitra");
}

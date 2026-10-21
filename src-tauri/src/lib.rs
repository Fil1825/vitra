//! Vitra desktop app: Tauri setup and command registration.

mod error;
mod settings;
mod sidecar;
mod window;

use tauri::Manager;

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = settings::SettingsStore::load(config_dir.join("settings.json"));
            let material = settings.get().appearance.window_material;
            app.manage(settings);

            let effects = window::WindowEffectsState::default();
            let main = window::create_from_config(app, "main")?;
            effects.apply(&main, material);
            app.manage(effects);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::set_settings,
            sidecar::sidecar_status,
            window::window_effects,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vitra");
}

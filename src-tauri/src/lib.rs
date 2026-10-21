//! Vitra desktop app: Tauri setup and command registration.

mod error;
mod player;
mod settings;
mod sidecar;
mod window;

use std::path::PathBuf;

use tauri::{Manager, RunEvent};

use player::Player;

/// Builds and runs the Tauri application.
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol(player::ARTWORK_SCHEME, |ctx, request| {
            match ctx.app_handle().try_state::<Player>() {
                Some(player) => player.artwork().respond(request.uri().path()),
                None => tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::SERVICE_UNAVAILABLE)
                    .body(Vec::new())
                    .unwrap_or_default(),
            }
        })
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = settings::SettingsStore::load(config_dir.join("settings.json"));
            let material = settings.get().appearance.window_material;
            app.manage(settings);

            let effects = window::WindowEffectsState::default();
            let main = window::create_from_config(app, "main")?;
            effects.apply(&main, material);
            app.manage(effects);

            let player = Player::new(app.handle().clone());
            player.start();
            // Files passed on the command line ("Open with" on Windows and Linux).
            let launch_files: Vec<PathBuf> = std::env::args_os()
                .skip(1)
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .collect();
            open_first(&player, launch_files);
            app.manage(player);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::set_settings,
            sidecar::sidecar_status,
            window::window_effects,
            player::player_state,
            player::player_open,
            player::player_play,
            player::player_pause,
            player::player_toggle,
            player::player_seek,
            player::player_set_volume,
            player::player_set_muted,
            player::player_set_speed,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Vitra");

    app.run(|app, event| match event {
        // "Open with" and dropping files on the Dock icon on macOS.
        #[cfg(target_os = "macos")]
        RunEvent::Opened { urls } => {
            let files = urls
                .iter()
                .filter_map(|url| url.to_file_path().ok())
                .collect();
            open_first(app.state::<Player>().inner(), files);
        }
        RunEvent::Exit => {
            // Quit mpv with the app; otherwise it would outlive us as an idle orphan.
            let player = app.state::<Player>().inner().clone();
            tauri::async_runtime::block_on(player.shutdown());
        }
        _ => {}
    });
}

/// Plays the first of the given files (the queue arrives in stage 3).
fn open_first(player: &Player, files: Vec<PathBuf>) {
    let Some(first) = files.into_iter().next() else {
        return;
    };
    let player = player.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = player.open(first).await {
            log::warn!("cannot open file from launch: {err}");
        }
    });
}

//! Vitra desktop app: Tauri setup and command registration.

mod drag;
mod error;
mod layout;
mod library;
mod links;
mod lyrics;
mod mini;
mod player;
mod settings;
mod sidecar;
mod window;

use std::path::PathBuf;

use tauri::http::StatusCode;
use tauri::{Manager, RunEvent};

use library::LibraryService;
use mini::MiniPlayer;
use player::Player;

/// Builds and runs the Tauri application.
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                // lofty warns about every odd MP3 frame while a library is scanned; the
                // scanner reports files it can't read itself.
                .level_for("lofty", log::LevelFilter::Error)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        // Artwork of the playing track (memory) and library album covers (disk), answered off
        // the main thread: a grid of covers means many requests at once.
        .register_asynchronous_uri_scheme_protocol(
            player::ARTWORK_SCHEME,
            |ctx, request, responder| {
                let app = ctx.app_handle().clone();
                let path = request.uri().path().to_owned();
                tauri::async_runtime::spawn_blocking(move || {
                    let response = match path.strip_prefix(player::COVER_PREFIX) {
                        Some(id) => app.try_state::<LibraryService>().map_or_else(
                            || player::http_status(StatusCode::SERVICE_UNAVAILABLE),
                            |library| player::respond_cover(library.covers_dir(), id),
                        ),
                        None => app.try_state::<Player>().map_or_else(
                            || player::http_status(StatusCode::SERVICE_UNAVAILABLE),
                            |player| player.artwork().respond(&path),
                        ),
                    };
                    responder.respond(response);
                });
            },
        )
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = settings::SettingsStore::load(config_dir.join("settings.json"));
            let material = settings.get().appearance.window_material;
            app.manage(settings);

            let effects = window::WindowEffectsState::default();
            let main = window::create_from_config(app.handle(), mini::MAIN)?;
            effects.apply(&main, window::Backdrop::plain(material));
            app.manage(effects);

            let mini = MiniPlayer::new(app.handle().clone(), &config_dir);
            let watcher = mini.clone();
            main.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    watcher.main_close_requested(api);
                }
            });
            // It was open when the app quit: the app opens in it again.
            if mini.was_open() {
                if app
                    .state::<settings::SettingsStore>()
                    .get()
                    .mini_player
                    .hide_main_window
                {
                    main.hide()?;
                }
                let mini = mini.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = mini.show().await {
                        log::warn!("cannot reopen the mini player: {err}");
                    }
                });
            }
            app.manage(mini);

            let data_dir = app.path().app_data_dir()?;
            let library = LibraryService::start(app.handle().clone(), &data_dir)?;
            // The settings file is the source of truth; the library keeps a copy for its
            // scanner.
            let splitting = app
                .state::<settings::SettingsStore>()
                .get()
                .library
                .artist_splitting;
            library.set_artist_splitting(splitting);
            app.manage(library);
            app.manage(lyrics::LyricsService::new(app.handle().clone(), &data_dir));
            let links = links::LinkService::new(app.handle().clone())?;
            links.warm_up();
            app.manage(links);
            app.manage(links::Downloads::new(app.handle().clone())?);

            let player = Player::new(app.handle().clone());
            player.start();
            // Files passed on the command line ("Open with" on Windows and Linux).
            let launch_files: Vec<PathBuf> = std::env::args_os()
                .skip(1)
                .map(PathBuf::from)
                .filter(|path| path.exists())
                .collect();
            open_files(&player, launch_files);
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
            player::player_level_meter,
            player::player_play,
            player::player_pause,
            player::player_toggle,
            player::player_next,
            player::player_previous,
            player::player_seek,
            player::player_set_volume,
            player::player_set_muted,
            player::player_set_speed,
            player::queue_get,
            player::queue_play,
            player::queue_add,
            player::queue_jump,
            player::queue_remove,
            player::queue_move,
            player::queue_clear,
            player::queue_set_shuffle,
            player::queue_set_repeat,
            library::library_folders,
            library::library_add_folder,
            library::library_remove_folder,
            library::library_rescan,
            library::library_scan_status,
            library::library_tracks,
            library::library_track,
            library::library_albums,
            library::library_album,
            library::library_artists,
            library::library_artist,
            library::library_artist_albums,
            library::library_search,
            library::library_set_favorite,
            library::library_remove_tracks,
            library::library_history,
            library::playlists_list,
            library::playlist_get,
            library::playlist_create,
            library::playlist_rename,
            library::playlist_delete,
            library::playlist_add,
            library::playlist_remove,
            library::playlist_move,
            library::tags_load,
            library::tags_save,
            library::artwork_preview,
            library::artwork_original,
            library::library_set_cover,
            links::link_resolve,
            links::link_play,
            links::link_import,
            links::link_restore,
            links::link_download,
            links::downloads_list,
            links::downloads_add,
            links::downloads_add_album,
            links::links_cookies_access,
            links::downloads_cancel,
            links::downloads_retry,
            links::downloads_cancel_group,
            links::downloads_retry_group,
            links::downloads_clear,
            links::downloads_folder,
            links::ytdlp_info,
            links::ytdlp_update,
            lyrics::lyrics_current,
            lyrics::lyrics_set_offset,
            lyrics::lyrics_search_online,
            lyrics::lyrics_get,
            lyrics::lyrics_parse,
            lyrics::lyrics_serialize,
            lyrics::lyrics_save,
            lyrics::lyrics_import,
            lyrics::lyrics_export,
            drag::drag_link,
            mini::mini_show,
            mini::mini_hide,
            mini::mini_toggle,
            mini::mini_expand,
            mini::app_quit,
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
            open_files(app.state::<Player>().inner(), files);
        }
        // The Dock icon brings the main window back from the mini player.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            if let Some(mini) = app.try_state::<MiniPlayer>()
                && let Err(err) = mini.switch_to_main()
            {
                log::warn!("cannot show the main window: {err}");
            }
        }
        RunEvent::Exit => {
            if let Some(library) = app.try_state::<LibraryService>() {
                library.stop();
            }
            // Quit mpv with the app; otherwise it would outlive us as an idle orphan.
            if let Some(player) = app.try_state::<Player>() {
                let player = player.inner().clone();
                tauri::async_runtime::block_on(player.shutdown());
            }
        }
        _ => {}
    });
}

/// Plays files given at launch or by the OS, replacing the queue.
fn open_files(player: &Player, files: Vec<PathBuf>) {
    if files.is_empty() {
        return;
    }
    let player = player.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = player.open(files).await {
            log::warn!("cannot open files from launch: {err}");
        }
    });
}

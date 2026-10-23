//! Themes the user installed or made: `.vitratheme` packages unpacked in
//! `<app data>/themes/<id>/` (see `crates/themes`), and their pictures and fonts served to the
//! windows through the `theme://` scheme. What a theme says (its schema, its CSS) is checked
//! by the frontend; this side keeps the files safe and in place.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Emitter, State};
use themes::Package;

use crate::error::AppError;
use crate::player::http_status;

/// `theme://localhost/<id>/<asset>` (`http://theme.localhost/…` on Windows).
pub const THEME_SCHEME: &str = "theme";

/// Emitted to every window after a theme was installed, saved or removed.
pub const THEMES_CHANGED: &str = "themes://changed";

/// Opened packages waiting for the frontend's verdict: a few at most.
const PENDING: usize = 4;

pub struct ThemeStore {
    dir: PathBuf,
    pending: Mutex<VecDeque<(u64, Package)>>,
    next: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTheme {
    pub id: String,
    /// `theme.json` as written; the frontend validates it.
    pub json: String,
    pub css: Option<String>,
    /// File names in the theme's `assets/` folder.
    pub assets: Vec<String>,
}

impl From<themes::Installed> for InstalledTheme {
    fn from(theme: themes::Installed) -> Self {
        InstalledTheme {
            id: theme.id,
            json: theme.theme,
            css: theme.css,
            assets: theme.assets,
        }
    }
}

/// A package read from a file, before it is installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedPackage {
    pub token: u64,
    pub json: String,
    pub css: Option<String>,
    pub assets: Vec<String>,
}

impl ThemeStore {
    pub fn new(dir: PathBuf) -> Self {
        ThemeStore {
            dir,
            pending: Mutex::new(VecDeque::new()),
            next: AtomicU64::new(1),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, VecDeque<(u64, Package)>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take(&self, token: u64) -> Option<Package> {
        let mut pending = self.pending();
        let index = pending.iter().position(|(known, _)| *known == token)?;
        pending.remove(index).map(|(_, package)| package)
    }

    /// Serves a theme's picture or font (`/<id>/<name>`); nothing else is reachable.
    pub fn respond(&self, path: &str) -> Response<Vec<u8>> {
        let mut parts = path.trim_start_matches('/').splitn(2, '/');
        let (Some(id), Some(name)) = (parts.next(), parts.next()) else {
            return http_status(StatusCode::BAD_REQUEST);
        };
        let Some((file, mime)) =
            themes::asset_path(&self.dir, id, name).zip(themes::mime_type(name))
        else {
            return http_status(StatusCode::BAD_REQUEST);
        };
        match std::fs::read(file) {
            // Re-importing a theme can change an asset under the same name.
            Ok(bytes) => Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "no-cache")
                .body(bytes)
                .unwrap_or_else(|_| http_status(StatusCode::INTERNAL_SERVER_ERROR)),
            Err(_) => http_status(StatusCode::NOT_FOUND),
        }
    }
}

async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| AppError::Task(err.to_string()))?
}

fn changed(app: &AppHandle) -> Result<(), AppError> {
    app.emit(THEMES_CHANGED, ())?;
    Ok(())
}

/// Installed themes.
#[tauri::command]
pub async fn themes_list(store: State<'_, ThemeStore>) -> Result<Vec<InstalledTheme>, AppError> {
    let dir = store.dir().to_path_buf();
    blocking(move || Ok(themes::list(&dir).into_iter().map(Into::into).collect())).await
}

/// Reads a `.vitratheme` file; the frontend checks it and then installs or discards it.
#[tauri::command]
pub async fn theme_open(
    store: State<'_, ThemeStore>,
    path: PathBuf,
) -> Result<OpenedPackage, AppError> {
    let package = blocking(move || Ok(themes::read_package(&path)?)).await?;
    let token = store.next.fetch_add(1, Ordering::Relaxed);
    let opened = OpenedPackage {
        token,
        json: package.theme.clone(),
        css: package.css.clone(),
        assets: package.assets.keys().cloned().collect(),
    };
    let mut pending = store.pending();
    pending.push_back((token, package));
    while pending.len() > PENDING {
        pending.pop_front();
    }
    Ok(opened)
}

/// Installs an opened package as theme `id` with the checked `theme.json` and `custom.css`
/// (the frontend may have changed its id and cleaned its CSS).
#[tauri::command]
pub async fn theme_install(
    app: AppHandle,
    store: State<'_, ThemeStore>,
    token: u64,
    id: String,
    json: String,
    css: Option<String>,
) -> Result<(), AppError> {
    let mut package = store
        .take(token)
        .ok_or_else(|| AppError::NotFound("the theme package is no longer open".into()))?;
    package.theme = json;
    package.css = css;
    let dir = store.dir().to_path_buf();
    blocking(move || Ok(themes::install(&dir, &id, &package)?)).await?;
    changed(&app)
}

/// Forgets an opened package that won't be installed.
#[tauri::command]
pub fn theme_discard(store: State<'_, ThemeStore>, token: u64) {
    store.take(token);
}

/// Saves a theme the user made or edited (its assets stay as they are).
#[tauri::command]
pub async fn theme_save(
    app: AppHandle,
    store: State<'_, ThemeStore>,
    id: String,
    json: String,
    css: Option<String>,
) -> Result<(), AppError> {
    let dir = store.dir().to_path_buf();
    blocking(move || Ok(themes::save(&dir, &id, &json, css.as_deref())?)).await?;
    changed(&app)
}

/// Makes theme `to` a copy of the installed theme `from` (CSS and assets included) with
/// `json` as its `theme.json`.
#[tauri::command]
pub async fn theme_duplicate(
    app: AppHandle,
    store: State<'_, ThemeStore>,
    from: String,
    to: String,
    json: String,
) -> Result<(), AppError> {
    let dir = store.dir().to_path_buf();
    blocking(move || Ok(themes::duplicate(&dir, &from, &to, &json)?)).await?;
    changed(&app)
}

/// Copies a picture or font into a theme; returns its path there (`assets/<name>`).
#[tauri::command]
pub async fn theme_add_asset(
    app: AppHandle,
    store: State<'_, ThemeStore>,
    id: String,
    path: PathBuf,
) -> Result<String, AppError> {
    let dir = store.dir().to_path_buf();
    let added = blocking(move || Ok(themes::add_asset(&dir, &id, &path)?)).await?;
    changed(&app)?;
    Ok(added)
}

/// Writes a `.vitratheme` file: the given `theme.json` and `custom.css`, plus the assets of
/// the installed theme `id` if there is one (built-in themes have none).
#[tauri::command]
pub async fn theme_export(
    store: State<'_, ThemeStore>,
    id: String,
    json: String,
    css: Option<String>,
    path: PathBuf,
) -> Result<(), AppError> {
    let dir = store.dir().to_path_buf();
    blocking(move || {
        let assets = if dir.join(&id).is_dir() {
            themes::read_installed(&dir, &id)?.assets
        } else {
            Default::default()
        };
        let package = Package {
            theme: json,
            css,
            assets,
        };
        Ok(themes::write_package(&path, &package)?)
    })
    .await
}

/// Removes an installed theme.
#[tauri::command]
pub async fn theme_remove(
    app: AppHandle,
    store: State<'_, ThemeStore>,
    id: String,
) -> Result<(), AppError> {
    let dir = store.dir().to_path_buf();
    blocking(move || Ok(themes::remove(&dir, &id)?)).await?;
    changed(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_assets_are_served() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ThemeStore::new(tmp.path().to_path_buf());
        themes::save(store.dir(), "mono", "{}", Some("a{}")).unwrap();
        let picture = tmp.path().join("bg.png");
        std::fs::write(&picture, b"png").unwrap();
        themes::add_asset(store.dir(), "mono", &picture).unwrap();

        let served = store.respond("/mono/bg.png");
        assert_eq!(served.status(), StatusCode::OK);
        assert_eq!(served.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(served.body(), b"png");
        for path in [
            "/mono/custom.css",
            "/mono/../mono/bg.png",
            "/mono",
            "/../x.png",
        ] {
            assert_eq!(
                store.respond(path).status(),
                StatusCode::BAD_REQUEST,
                "{path}"
            );
        }
        assert_eq!(
            store.respond("/mono/none.png").status(),
            StatusCode::NOT_FOUND
        );
    }
}

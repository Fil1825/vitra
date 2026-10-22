//! Locating the external helper binaries Vitra drives: mpv, yt-dlp, FFmpeg and Deno.
//!
//! Binaries are looked up in priority order:
//! 1. `<app data>/sidecars/`: user-updatable copies (yt-dlp updates itself there).
//! 2. Bundled with the app: next to the executable and in `<resources>/sidecars/`.
//! 3. Debug builds only: `src-tauri/sidecars/<target triple>/`, filled by
//!    `scripts/fetch-sidecars.mjs`.
//! 4. The system: `PATH` plus well-known package manager directories, because
//!    GUI apps on macOS do not inherit the shell's `PATH`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// An external program Vitra runs as a child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sidecar {
    Mpv,
    YtDlp,
    Ffmpeg,
    Ffprobe,
    Deno,
}

impl Sidecar {
    pub const ALL: [Sidecar; 5] = [
        Sidecar::Mpv,
        Sidecar::YtDlp,
        Sidecar::Ffmpeg,
        Sidecar::Ffprobe,
        Sidecar::Deno,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Sidecar::Mpv => "mpv",
            Sidecar::YtDlp => "yt-dlp",
            Sidecar::Ffmpeg => "ffmpeg",
            Sidecar::Ffprobe => "ffprobe",
            Sidecar::Deno => "deno",
        }
    }

    /// Executable file name on the current platform.
    pub fn file_name(self) -> String {
        format!("{}{}", self.name(), std::env::consts::EXE_SUFFIX)
    }

    /// Candidate locations of the executable relative to a sidecar root.
    ///
    /// mpv ships with shared libraries, so `fetch-sidecars` keeps it in its own directory.
    /// So does yt-dlp on macOS: the single-file build unpacks Python on every run and macOS
    /// checks those fresh libraries each time (~25 s), so only the folder build counts there.
    fn layouts(self) -> Vec<PathBuf> {
        match self {
            Sidecar::Mpv if cfg!(target_os = "macos") => vec![
                PathBuf::from("mpv/mpv.app/Contents/MacOS/mpv"),
                PathBuf::from(self.file_name()),
            ],
            Sidecar::Mpv => vec![
                Path::new("mpv").join(self.file_name()),
                PathBuf::from(self.file_name()),
            ],
            Sidecar::YtDlp if cfg!(target_os = "macos") => vec![PathBuf::from(YT_DLP_MACOS)],
            _ => vec![PathBuf::from(self.file_name())],
        }
    }
}

/// yt-dlp's folder build on macOS, relative to a sidecar root (`scripts/sidecars/plan.mjs`).
pub const YT_DLP_MACOS: &str = "yt-dlp/yt-dlp_macos";

/// Where a sidecar was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SidecarSource {
    AppData,
    Bundled,
    Dev,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub path: PathBuf,
    pub source: SidecarSource,
}

/// Ordered list of directories to search for sidecars.
#[derive(Debug, Default, Clone)]
pub struct SearchPaths {
    roots: Vec<(PathBuf, SidecarSource)>,
    system_dirs: Vec<PathBuf>,
}

impl SearchPaths {
    /// Adds a sidecar root; roots are searched in the order they were added.
    pub fn with_root(mut self, dir: impl Into<PathBuf>, source: SidecarSource) -> Self {
        self.roots.push((dir.into(), source));
        self
    }

    /// Adds the directories of a `PATH`-style variable, then well-known install directories.
    pub fn with_system_path(mut self, path_var: Option<OsString>) -> Self {
        if let Some(path_var) = path_var {
            self.system_dirs.extend(std::env::split_paths(&path_var));
        }
        for dir in well_known_dirs() {
            let dir = PathBuf::from(dir);
            if !self.system_dirs.contains(&dir) {
                self.system_dirs.push(dir);
            }
        }
        self
    }

    pub fn locate(&self, sidecar: Sidecar) -> Option<Located> {
        let bundled = self.roots.iter().flat_map(|(root, source)| {
            sidecar
                .layouts()
                .into_iter()
                .map(move |layout| (root.join(layout), *source))
        });
        let system = self
            .system_dirs
            .iter()
            .map(|dir| (dir.join(sidecar.file_name()), SidecarSource::System));

        bundled
            .chain(system)
            .find(|(path, _)| is_executable(path))
            .map(|(path, source)| Located { path, source })
    }
}

fn well_known_dirs() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
    } else if cfg!(target_os = "linux") {
        &["/usr/local/bin", "/usr/bin"]
    } else {
        &[]
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Search paths for the running app, in the priority order described in the module docs.
pub fn search_paths(app: &AppHandle) -> SearchPaths {
    let mut paths = SearchPaths::default();
    if let Ok(dir) = app.path().app_data_dir() {
        paths = paths.with_root(dir.join("sidecars"), SidecarSource::AppData);
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        paths = paths.with_root(dir, SidecarSource::Bundled);
    }
    if let Ok(dir) = app.path().resource_dir() {
        paths = paths.with_root(dir.join("sidecars"), SidecarSource::Bundled);
    }
    if cfg!(debug_assertions) {
        let dev_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sidecars")
            .join(env!("VITRA_TARGET_TRIPLE"));
        paths = paths.with_root(dev_dir, SidecarSource::Dev);
    }
    paths.with_system_path(std::env::var_os("PATH"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatus {
    name: &'static str,
    path: Option<String>,
    source: Option<SidecarSource>,
}

/// Reports where each sidecar was found, for diagnostics in the UI.
#[tauri::command]
pub fn sidecar_status(app: AppHandle) -> Vec<SidecarStatus> {
    let paths = search_paths(&app);
    Sidecar::ALL
        .iter()
        .map(|&sidecar| {
            let located = paths.locate(sidecar);
            SidecarStatus {
                name: sidecar.name(),
                path: located
                    .as_ref()
                    .map(|l| l.path.to_string_lossy().into_owned()),
                source: located.map(|l| l.source),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch_executable(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn earlier_roots_win() {
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path().join("app-data");
        let dev = tmp.path().join("dev");
        let name = Sidecar::YtDlp.file_name();
        touch_executable(&app_data.join(&name));
        touch_executable(&dev.join(&name));

        let paths = SearchPaths::default()
            .with_root(&app_data, SidecarSource::AppData)
            .with_root(&dev, SidecarSource::Dev);

        assert_eq!(
            paths.locate(Sidecar::YtDlp),
            Some(Located {
                path: app_data.join(&name),
                source: SidecarSource::AppData,
            })
        );
    }

    #[test]
    fn mpv_is_found_in_its_own_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let expected = tmp.path().join(&Sidecar::Mpv.layouts()[0]);
        touch_executable(&expected);

        let paths = SearchPaths::default().with_root(tmp.path(), SidecarSource::Bundled);

        assert_eq!(paths.locate(Sidecar::Mpv).map(|l| l.path), Some(expected));
    }

    #[test]
    fn yt_dlp_on_macos_is_the_folder_build() {
        let single = tempfile::tempdir().unwrap();
        touch_executable(&single.path().join(Sidecar::YtDlp.file_name()));
        let folder = tempfile::tempdir().unwrap();
        touch_executable(&folder.path().join(YT_DLP_MACOS));
        let found = |root: &Path| {
            SearchPaths::default()
                .with_root(root, SidecarSource::AppData)
                .locate(Sidecar::YtDlp)
                .map(|l| l.path)
        };
        if cfg!(target_os = "macos") {
            // A single-file copy left from before is ignored: it would be the slow one.
            assert_eq!(found(single.path()), None);
            assert_eq!(found(folder.path()), Some(folder.path().join(YT_DLP_MACOS)));
        } else {
            assert_eq!(
                found(single.path()),
                Some(single.path().join(Sidecar::YtDlp.file_name()))
            );
            assert_eq!(found(folder.path()), None);
        }
    }

    #[test]
    fn falls_back_to_system_path() {
        let tmp = tempfile::tempdir().unwrap();
        let empty_root = tmp.path().join("bundled");
        let bin = tmp.path().join("bin");
        touch_executable(&bin.join(Sidecar::Ffmpeg.file_name()));

        let path_var = std::env::join_paths([&bin]).unwrap();
        let paths = SearchPaths::default()
            .with_root(&empty_root, SidecarSource::Bundled)
            .with_system_path(Some(path_var));

        let located = paths.locate(Sidecar::Ffmpeg).unwrap();
        assert_eq!(located.source, SidecarSource::System);
        assert_eq!(located.path, bin.join(Sidecar::Ffmpeg.file_name()));
    }

    #[test]
    fn missing_sidecar_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = SearchPaths::default().with_root(tmp.path(), SidecarSource::Bundled);
        assert_eq!(paths.locate(Sidecar::Deno), None);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_files_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(Sidecar::Ffprobe.file_name());
        fs::write(&path, b"").unwrap();

        let paths = SearchPaths::default().with_root(tmp.path(), SidecarSource::Bundled);
        assert_eq!(paths.locate(Sidecar::Ffprobe), None);
    }
}

//! Whether Vitra can read a browser's cookies, asked before yt-dlp tries (and fails in the
//! middle of playing a link).

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CookiesAccess {
    Ok,
    /// The system keeps Vitra out (macOS: Safari's cookies need Full Disk Access).
    NoAccess,
    /// Not something that can be checked cheaply (Chrome asks the Keychain when yt-dlp runs).
    Unknown,
}

/// Can Vitra read `browser`'s cookies? Only Safari's can be checked without running yt-dlp:
/// they are a file macOS protects, so opening it fails with "Operation not permitted".
pub fn access(browser: &str) -> CookiesAccess {
    match (browser, safari_cookie_files()) {
        ("safari", Some(files)) => access_of(&files, |path| std::fs::File::open(path).map(drop)),
        _ => CookiesAccess::Unknown,
    }
}

/// Where yt-dlp looks for Safari's cookies, in its order.
fn safari_cookie_files() -> Option<Vec<PathBuf>> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(vec![
        home.join("Library/Cookies/Cookies.binarycookies"),
        home.join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies"),
    ])
}

/// The first file that is there decides; `open` tries to read one.
fn access_of(files: &[PathBuf], open: impl Fn(&Path) -> io::Result<()>) -> CookiesAccess {
    for file in files {
        match open(file) {
            Ok(()) => return CookiesAccess::Ok,
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                return CookiesAccess::NoAccess;
            }
            Err(_) => {}
        }
    }
    CookiesAccess::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_cookie_file_there_decides() {
        let files = [PathBuf::from("/a"), PathBuf::from("/b")];
        let missing = || io::Error::from(io::ErrorKind::NotFound);
        // Unix EPERM and Windows ERROR_ACCESS_DENIED use different OS codes.
        let denied = || io::Error::from_raw_os_error(if cfg!(windows) { 5 } else { 1 });
        assert_eq!(denied().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            access_of(&files, |path| if path == Path::new("/a") {
                Err(missing())
            } else {
                Err(denied())
            }),
            CookiesAccess::NoAccess
        );
        assert_eq!(access_of(&files, |_| Ok(())), CookiesAccess::Ok);
        assert_eq!(
            access_of(&files, |_| Err(missing())),
            CookiesAccess::Unknown
        );
        assert_eq!(access("chrome"), CookiesAccess::Unknown);
    }
}

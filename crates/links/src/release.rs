//! Installing a yt-dlp release that can't update itself: the macOS folder build ("Auto-update
//! is not supported for unpackaged executables"). The zip and the published checksums are
//! fetched with curl, which every macOS (and Windows 10+) has, so Vitra needs no HTTP client.

use std::fs;
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::{LinkError, Result};

/// yt-dlp's latest release.
pub const LATEST_RELEASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
/// The checksums published with every release.
const SUMS: &str = "SHA2-256SUMS";
/// Written into an installed folder: the hash of the zip it came from, so installing the
/// same release again is skipped.
const MARKER: &str = ".release-sha256";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const UNPACK_TIMEOUT: Duration = Duration::from_secs(120);

/// The SHA-256 of `asset` in a `sha256sum`-style file (`<hash>  <name>` or `<hash> *<name>`).
pub fn sha256_in_sums(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// A command that doesn't flash a console window on Windows.
fn quiet(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

async fn run(mut command: Command, what: &str, timeout: Duration) -> Result<()> {
    let child = command
        .spawn()
        .map_err(|err| LinkError::Failed(format!("cannot run {what}: {err}")))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| LinkError::TimedOut)??;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.lines().map(str::trim).rfind(|l| !l.is_empty());
    Err(LinkError::Failed(match detail {
        Some(detail) => format!("{what} failed: {detail}"),
        None => format!("{what} failed ({})", output.status),
    }))
}

async fn download(url: &str, to: &Path) -> Result<()> {
    let mut curl = quiet(if cfg!(windows) { "curl.exe" } else { "curl" });
    curl.args(["-fsSL", "--retry", "2", "--connect-timeout", "30", "-o"])
        .arg(to)
        .arg(url);
    run(curl, "curl", DOWNLOAD_TIMEOUT).await
}

/// Unpacks with the system's own tool: `ditto` keeps what macOS needs (permissions,
/// extended attributes); Windows' bsdtar reads zip; Linux has `unzip`.
async fn unzip(zip: &Path, into: &Path) -> Result<()> {
    fs::create_dir_all(into)?;
    let command = if cfg!(target_os = "macos") {
        let mut ditto = quiet("/usr/bin/ditto");
        ditto.args(["-x", "-k"]).arg(zip).arg(into);
        ditto
    } else if cfg!(windows) {
        let mut tar = quiet("tar.exe");
        tar.arg("-xf").arg(zip).arg("-C").arg(into);
        tar
    } else {
        let mut unzip = quiet("unzip");
        unzip.args(["-q", "-o"]).arg(zip).arg("-d").arg(into);
        unzip
    };
    run(command, "unzipping", UNPACK_TIMEOUT).await
}

fn sha256_of(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn remove(path: &Path) {
    let _ = fs::remove_dir_all(path).or_else(|_| fs::remove_file(path));
}

/// Puts `new` in place of `dir` (a folder, or a file left from an older layout); the old one
/// comes back if the move fails.
fn swap_in(new: &Path, dir: &Path, aside: &Path) -> Result<()> {
    remove(aside);
    let had_old = dir.symlink_metadata().is_ok();
    if had_old {
        fs::rename(dir, aside)?;
    }
    if let Err(err) = fs::rename(new, dir) {
        if had_old {
            let _ = fs::rename(aside, dir);
        }
        return Err(err.into());
    }
    remove(aside);
    Ok(())
}

/// Installs the zipped release asset `<base>/<asset>` into `dir` (its contents become the
/// folder's), after checking it against the release's `SHA2-256SUMS`. Programs still running
/// from the old folder keep working (it is moved aside, then deleted). Returns `false` when
/// `dir` already holds exactly that release.
pub async fn install_zip_release(base: &str, asset: &str, dir: &Path) -> Result<bool> {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
        return Err(LinkError::Failed(format!("bad folder {}", dir.display())));
    };
    let name = name.to_string_lossy();
    let pid = std::process::id();
    // Next to `dir`, so the final moves are renames on one file system.
    let work = parent.join(format!(".{name}-update-{pid}"));
    remove(&work);
    fs::create_dir_all(&work)?;

    let result = async {
        let sums = work.join(SUMS);
        download(&format!("{base}/{SUMS}"), &sums).await?;
        let expected = sha256_in_sums(&fs::read_to_string(&sums)?, asset)
            .ok_or_else(|| LinkError::Failed(format!("no checksum for {asset} in {SUMS}")))?;
        let installed = fs::read_to_string(dir.join(MARKER)).unwrap_or_default();
        if installed.trim() == expected {
            return Ok(false);
        }

        let zip = work.join(asset);
        download(&format!("{base}/{asset}"), &zip).await?;
        let hash = {
            let zip = zip.clone();
            tokio::task::spawn_blocking(move || sha256_of(&zip))
                .await
                .map_err(|err| LinkError::Failed(err.to_string()))??
        };
        if hash != expected {
            return Err(LinkError::Failed(format!(
                "{asset} doesn't match its checksum (expected {expected}, got {hash})"
            )));
        }
        let unpacked = work.join("unpacked");
        unzip(&zip, &unpacked).await?;
        fs::write(unpacked.join(MARKER), &expected)?;
        swap_in(&unpacked, dir, &parent.join(format!(".{name}-old-{pid}")))?;
        Ok(true)
    }
    .await;
    remove(&work);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_asset_in_the_checksums() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  yt-dlp_macos\n{b} *yt-dlp_macos.zip\nnot a line\n");
        assert_eq!(sha256_in_sums(&sums, "yt-dlp_macos"), Some(a));
        assert_eq!(
            sha256_in_sums(&sums, "yt-dlp_macos.zip"),
            Some("b".repeat(64))
        );
        assert_eq!(sha256_in_sums(&sums, "yt-dlp_linux"), None);
        assert_eq!(
            sha256_in_sums("abc  yt-dlp_macos.zip", "yt-dlp_macos.zip"),
            None
        );
    }

    #[test]
    fn swapping_replaces_a_file_or_folder_and_keeps_the_old_one_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("yt-dlp");
        let aside = tmp.path().join(".yt-dlp-old");
        // An older single-file layout at the same path.
        fs::write(&dir, b"old program").unwrap();
        let new = tmp.path().join("new");
        fs::create_dir_all(new.join("_internal")).unwrap();
        swap_in(&new, &dir, &aside).unwrap();
        assert!(dir.join("_internal").is_dir());
        assert!(!aside.exists() && !new.exists());

        // Nothing to move in: the installed folder stays.
        assert!(swap_in(&tmp.path().join("missing"), &dir, &aside).is_err());
        assert!(dir.join("_internal").is_dir());
    }
}

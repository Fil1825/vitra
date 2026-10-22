//! Installs a yt-dlp release zip served from this test, with the system's curl and unzip
//! (`ditto` on macOS).

mod common;

use std::path::Path;

use links::release::install_zip_release;
use sha2::{Digest, Sha256};

const ASSET: &str = "yt-dlp_macos.zip";

fn zip() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yt-dlp-release.zip"))
        .unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn curl_available() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Serves the zip with the given checksum file.
fn release(sums: String) -> String {
    let zip = zip();
    common::serve(move |_| {
        vec![
            ("/SHA2-256SUMS", sums.into_bytes(), "text/plain"),
            ("/yt-dlp_macos.zip", zip, "application/zip"),
        ]
    })
}

#[tokio::test]
async fn installs_a_release_once() {
    if !curl_available() {
        return;
    }
    let base = release(format!("{}  {ASSET}\n", sha256(&zip())));
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("sidecars/yt-dlp");
    // A single-file yt-dlp left from before sits where the folder goes.
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::write(&dir, b"old single-file build").unwrap();

    assert!(install_zip_release(&base, ASSET, &dir).await.unwrap());
    assert!(dir.join("_internal/library.txt").is_file());
    let program = dir.join("yt-dlp_macos");
    assert!(program.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&program).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "still executable");
    }
    // Only the folder is left next to it: no download or old copy.
    let names: Vec<_> = std::fs::read_dir(dir.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["yt-dlp"]);

    // The same release again is skipped.
    assert!(!install_zip_release(&base, ASSET, &dir).await.unwrap());
}

#[tokio::test]
async fn refuses_a_zip_that_does_not_match_its_checksum() {
    if !curl_available() {
        return;
    }
    let base = release(format!("{}  {ASSET}\n", "0".repeat(64)));
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("yt-dlp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("yt-dlp_macos"), b"installed").unwrap();

    let error = install_zip_release(&base, ASSET, &dir).await.unwrap_err();
    assert!(error.to_string().contains("checksum"), "{error}");
    assert_eq!(
        std::fs::read(dir.join("yt-dlp_macos")).unwrap(),
        b"installed"
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);

    // A release without this asset is an error too.
    let base = release(String::new());
    let error = install_zip_release(&base, ASSET, &dir).await.unwrap_err();
    assert!(error.to_string().contains("no checksum"), "{error}");
}

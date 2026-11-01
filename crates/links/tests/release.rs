//! Installs a yt-dlp release served from this test, with the system's curl and unzip
//! (`ditto` on macOS). The checksum file is signed by a key this test holds. The production
//! entry points pin yt-dlp's release key, so a signature from this key must not install.

mod common;

use std::path::Path;

use links::release::{
    install_file_release, install_file_release_with_key, install_zip_release,
    install_zip_release_with_key,
};
use pgp::composed::{
    DetachedSignature, KeyType, SecretKeyParamsBuilder, SignedPublicKey, SignedSecretKey,
};
use pgp::crypto::hash::HashAlgorithm;
use pgp::ser::Serialize;
use pgp::types::Password;
use sha2::{Digest, Sha256};

const ASSET: &str = "yt-dlp_macos.zip";
const ZIP_PATH: &str = "/yt-dlp_macos.zip";
const FILE_ASSET: &str = "yt-dlp_linux";
const FILE_PATH: &str = "/yt-dlp_linux";

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

fn test_keypair() -> (SignedSecretKey, SignedPublicKey) {
    let mut params = SecretKeyParamsBuilder::default();
    params
        .key_type(KeyType::Ed25519Legacy)
        .can_certify(true)
        .can_sign(true)
        .primary_user_id("Vitra test <test@example.com>".into());
    let secret = params
        .build()
        .unwrap()
        .generate(rand::thread_rng())
        .unwrap();
    let public = secret.to_public_key();
    (secret, public)
}

fn sign(secret: &SignedSecretKey, data: &[u8]) -> Vec<u8> {
    let signature = DetachedSignature::sign_binary_data(
        rand::thread_rng(),
        &secret.primary_key,
        &Password::empty(),
        HashAlgorithm::Sha256,
        data,
    )
    .unwrap();
    signature.to_bytes().unwrap()
}

/// Serves the release files. `signature` is omitted to test a missing signature.
fn serve_release(
    sums: Vec<u8>,
    signature: Option<Vec<u8>>,
    asset_path: &'static str,
    body: Vec<u8>,
) -> String {
    common::serve(move |_| {
        let mut routes = vec![
            ("/SHA2-256SUMS", sums, "text/plain"),
            (asset_path, body, "application/octet-stream"),
        ];
        if let Some(signature) = signature {
            routes.push(("/SHA2-256SUMS.sig", signature, "application/octet-stream"));
        }
        routes
    })
}

#[tokio::test]
async fn installs_a_release_once_when_the_signature_matches() {
    if !curl_available() {
        return;
    }
    let (secret, public) = test_keypair();
    let bytes = zip();
    let sums = format!("{}  {ASSET}\n", sha256(&bytes));
    let signature = sign(&secret, sums.as_bytes());
    let base = serve_release(sums.into_bytes(), Some(signature), ZIP_PATH, bytes);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("sidecars/yt-dlp");
    // A single-file yt-dlp left from before sits where the folder goes.
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::write(&dir, b"old single-file build").unwrap();

    assert!(
        install_zip_release_with_key(&base, ASSET, &dir, &public)
            .await
            .unwrap()
    );
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

    // The same release again is skipped, still only after the signature checks out.
    assert!(
        !install_zip_release_with_key(&base, ASSET, &dir, &public)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn installs_a_single_file_release_when_the_signature_matches() {
    if !curl_available() {
        return;
    }
    let (secret, public) = test_keypair();
    let bytes = b"#!/bin/sh\necho yt-dlp\n".to_vec();
    let sums = format!("{}  {FILE_ASSET}\n", sha256(&bytes));
    let signature = sign(&secret, sums.as_bytes());
    let base = serve_release(sums.into_bytes(), Some(signature), FILE_PATH, bytes);
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("sidecars/yt-dlp");

    assert!(
        install_file_release_with_key(&base, FILE_ASSET, &file, &public)
            .await
            .unwrap()
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"#!/bin/sh\necho yt-dlp\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
    assert!(
        !install_file_release_with_key(&base, FILE_ASSET, &file, &public)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn refuses_a_zip_that_does_not_match_its_signed_checksum() {
    if !curl_available() {
        return;
    }
    let (secret, public) = test_keypair();
    let sums = format!("{}  {ASSET}\n", "0".repeat(64));
    let signature = sign(&secret, sums.as_bytes());
    let base = serve_release(sums.into_bytes(), Some(signature), ZIP_PATH, zip());
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("yt-dlp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("yt-dlp_macos"), b"installed").unwrap();

    let error = install_zip_release_with_key(&base, ASSET, &dir, &public)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("signed checksum"), "{error}");
    assert_eq!(
        std::fs::read(dir.join("yt-dlp_macos")).unwrap(),
        b"installed"
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);

    // A signed release without this asset is an error too.
    let empty = String::new();
    let signature = sign(&secret, empty.as_bytes());
    let base = serve_release(empty.into_bytes(), Some(signature), ZIP_PATH, zip());
    let error = install_zip_release_with_key(&base, ASSET, &dir, &public)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no checksum"), "{error}");
    assert_eq!(
        std::fs::read(dir.join("yt-dlp_macos")).unwrap(),
        b"installed"
    );
}

#[tokio::test]
async fn refuses_a_tampered_checksum_file_even_when_the_archive_matches_it() {
    if !curl_available() {
        return;
    }
    let (secret, public) = test_keypair();
    let bytes = zip();
    let sums = format!("{}  {ASSET}\n", sha256(&bytes));
    let signature = sign(&secret, sums.as_bytes());
    // The hash line still describes the zip. Only the signed bytes changed.
    let mut tampered = sums.into_bytes();
    tampered.push(b'\n');
    let base = serve_release(tampered, Some(signature), ZIP_PATH, bytes);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("yt-dlp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("yt-dlp_macos"), b"installed").unwrap();

    let error = install_zip_release_with_key(&base, ASSET, &dir, &public)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("not signed by the release key"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(dir.join("yt-dlp_macos")).unwrap(),
        b"installed"
    );
}

#[tokio::test]
async fn refuses_a_release_with_no_signature_and_a_signature_from_another_key() {
    if !curl_available() {
        return;
    }
    let (secret, public) = test_keypair();
    let bytes = zip();
    let sums = format!("{}  {ASSET}\n", sha256(&bytes));
    let base = serve_release(sums.clone().into_bytes(), None, ZIP_PATH, bytes.clone());
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("yt-dlp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("yt-dlp_macos"), b"installed").unwrap();

    let error = install_zip_release_with_key(&base, ASSET, &dir, &public)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("SHA2-256SUMS.sig"), "{error}");
    assert_eq!(
        std::fs::read(dir.join("yt-dlp_macos")).unwrap(),
        b"installed"
    );

    // A real signature, but not from the key the installer was given: the production
    // function pins yt-dlp's key, so this test key must not be enough.
    let signature = sign(&secret, sums.as_bytes());
    let base = serve_release(sums.into_bytes(), Some(signature), ZIP_PATH, bytes);
    let error = install_zip_release(&base, ASSET, &dir).await.unwrap_err();
    assert!(
        error.to_string().contains("not signed by the release key"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(dir.join("yt-dlp_macos")).unwrap(),
        b"installed"
    );

    // The single-file installer pins the same key.
    let error = install_file_release(&base, FILE_ASSET, &dir.join("unused"))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("not signed by the release key"),
        "{error}"
    );
}

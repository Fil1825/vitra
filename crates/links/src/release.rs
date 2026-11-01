//! Installing a yt-dlp release.
//!
//! `SHA2-256SUMS` is published next to the archive, so matching it only shows that the
//! archive and the checksum file came from the same place. Whoever can replace one can
//! replace the other. yt-dlp also publishes `SHA2-256SUMS.sig`, a detached signature made
//! with its release key. This module embeds that public key and refuses to install unless
//! the signature verifies. There is no unsigned fallback and no `yt-dlp -U`: that updater
//! downloads a new binary on its own, which would skip this check.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::types::KeyDetails;
use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::{LinkError, Result};

/// yt-dlp's latest release.
pub const LATEST_RELEASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
/// The checksums published with every release.
const SUMS: &str = "SHA2-256SUMS";
/// Detached signature over [`SUMS`], made with the key in `ytdlp_release_key.asc`.
const SUMS_SIG: &str = "SHA2-256SUMS.sig";
/// Written next to an installed release: the hash of the file it came from, so installing
/// the same release again is skipped.
const MARKER: &str = ".release-sha256";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const UNPACK_TIMEOUT: Duration = Duration::from_secs(120);

/// Primary fingerprint of [`YTDLP_RELEASE_KEY`].
///
/// The armored key is `public.key` from the yt-dlp repository, the file the project's
/// README tells users to verify `SHA2-256SUMS.sig` with:
/// <https://github.com/yt-dlp/yt-dlp/blob/master/public.key>
/// (retrieved 2026-10-31). Identity: Simon Sawicki (yt-dlp signing key)
/// <contact@grub4k.xyz>. A key rotation is a code change: replace the armored block and
/// this fingerprint together. A mismatch refuses every install.
const YTDLP_RELEASE_FINGERPRINT: &str = "ac0cbbe6848d6a873464af4e57cf65933b5a7581";
const YTDLP_RELEASE_KEY: &str = include_str!("ytdlp_release_key.asc");

/// Which release file this operating system installs.
///
/// The names are the ones in `SHA2-256SUMS` and match `scripts/sidecars/plan.mjs`.
/// On Linux the asset is not called `yt-dlp`: the standalone build is `yt-dlp_linux`
/// (or `yt-dlp_linux_aarch64`) and is saved as `yt-dlp` when it is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRelease {
    /// A zip whose contents become the destination directory.
    Zip { asset: &'static str },
    /// One file. `asset` is its name in the checksum file.
    File { asset: &'static str },
}

impl HostRelease {
    pub fn asset(self) -> &'static str {
        match self {
            Self::Zip { asset } | Self::File { asset } => asset,
        }
    }
}

/// The yt-dlp build Vitra installs on this machine, if upstream publishes one for it.
pub fn host_release() -> Result<HostRelease> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64" | "x86_64") => HostRelease::Zip {
            asset: "yt-dlp_macos.zip",
        },
        ("windows", "x86_64") => HostRelease::File {
            asset: "yt-dlp.exe",
        },
        ("linux", "x86_64") => HostRelease::File {
            asset: "yt-dlp_linux",
        },
        ("linux", "aarch64") => HostRelease::File {
            asset: "yt-dlp_linux_aarch64",
        },
        (os, arch) => {
            return Err(LinkError::Failed(format!(
                "no signed yt-dlp release is published for {os} {arch}"
            )));
        }
    })
}

/// The SHA-256 of `asset` in a `sha256sum`-style file (`<hash>  <name>` or `<hash> *<name>`).
pub fn sha256_in_sums(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// yt-dlp's release key. The fingerprint is pinned so a rewritten armored block does not
/// silently become the key we trust.
fn official_key() -> Result<SignedPublicKey> {
    let (key, _) = SignedPublicKey::from_string(YTDLP_RELEASE_KEY).map_err(|err| {
        LinkError::Failed(format!(
            "embedded yt-dlp release key is unreadable ({err}); refusing to install"
        ))
    })?;
    key.verify_bindings().map_err(|err| {
        LinkError::Failed(format!(
            "embedded yt-dlp release key failed its self-signature check ({err}); refusing to install"
        ))
    })?;
    let fingerprint = key.fingerprint().to_string();
    if fingerprint != YTDLP_RELEASE_FINGERPRINT {
        return Err(LinkError::Failed(format!(
            "embedded yt-dlp release key fingerprint is {fingerprint}, not {YTDLP_RELEASE_FINGERPRINT}; refusing to install"
        )));
    }
    Ok(key)
}

/// Confirms `sums` is exactly the byte string `signature` covers, under `key`.
///
/// An empty or unreadable signature is a failure. Verification is not skipped.
fn verify_signed_checksums(key: &SignedPublicKey, sums: &[u8], signature: &[u8]) -> Result<()> {
    if signature.is_empty() {
        return Err(LinkError::Failed(
            "yt-dlp release is missing its checksum signature; refusing to install".into(),
        ));
    }
    let signature = read_signature(signature)?;
    signature.verify(key, sums).map_err(|err| {
        LinkError::Failed(format!(
            "yt-dlp checksum file is not signed by the release key ({err}); refusing to install"
        ))
    })?;
    Ok(())
}

fn read_signature(bytes: &[u8]) -> Result<DetachedSignature> {
    let unreadable = |err: pgp::errors::Error| {
        LinkError::Failed(format!(
            "yt-dlp checksum signature could not be read ({err}); refusing to install"
        ))
    };
    // GitHub publishes a binary signature. Accept armor too, in case that changes.
    if bytes.starts_with(b"-----BEGIN") {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            LinkError::Failed(
                "yt-dlp checksum signature is not valid text; refusing to install".into(),
            )
        })?;
        let (signature, _) = DetachedSignature::from_string(text).map_err(unreadable)?;
        return Ok(signature);
    }
    DetachedSignature::from_bytes(Cursor::new(bytes)).map_err(unreadable)
}

/// The hash `asset` must have, once `sums` has been authenticated by `signature`.
fn signed_asset_digest(
    key: &SignedPublicKey,
    sums: &[u8],
    signature: &[u8],
    asset: &str,
) -> Result<String> {
    verify_signed_checksums(key, sums, signature)?;
    let text = std::str::from_utf8(sums).map_err(|_| {
        LinkError::Failed("yt-dlp checksum file is not UTF-8; refusing to install".into())
    })?;
    sha256_in_sums(text, asset).ok_or_else(|| {
        LinkError::Failed(format!(
            "no checksum for {asset} in the signed {SUMS}; refusing to install"
        ))
    })
}

fn require_digest(asset: &str, expected: &str, actual: &str) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(LinkError::Failed(format!(
        "{asset} doesn't match its signed checksum (expected {expected}, got {actual}); refusing to install"
    )))
}

fn single_component(asset: &str) -> Result<()> {
    let bad =
        asset.is_empty() || asset.contains(['/', '\\', '\0']) || asset == "." || asset == "..";
    if bad {
        return Err(LinkError::Failed(format!(
            "refusing yt-dlp release name {asset:?}"
        )));
    }
    Ok(())
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

async fn download_named(base: &str, name: &str, to: &Path) -> Result<()> {
    download(&format!("{base}/{name}"), to)
        .await
        .map_err(|err| LinkError::Failed(format!("cannot download yt-dlp {name}: {err}")))
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

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[derive(Clone, Copy)]
enum InstallKind {
    /// `dest` is a directory; the asset is a zip whose contents become it.
    Zip,
    /// `dest` is the installed file itself.
    File,
}

fn marker_path(dest: &Path, kind: InstallKind) -> Result<PathBuf> {
    match kind {
        // Inside the folder, so it is replaced in the same rename as the build.
        InstallKind::Zip => Ok(dest.join(MARKER)),
        InstallKind::File => {
            let name = dest
                .file_name()
                .ok_or_else(|| LinkError::Failed(format!("bad file {}", dest.display())))?;
            Ok(dest.with_file_name(format!(".{}{MARKER}", name.to_string_lossy())))
        }
    }
}

fn already_installed(dest: &Path, kind: InstallKind, expected: &str) -> bool {
    let Ok(marker) = marker_path(dest, kind) else {
        return false;
    };
    let installed = fs::read_to_string(marker).unwrap_or_default();
    if installed.trim() != expected {
        return false;
    }
    match kind {
        InstallKind::Zip => dest.is_dir(),
        InstallKind::File => dest.is_file(),
    }
}

/// Installs the zipped release asset `<base>/<asset>` into `dir` (its contents become the
/// folder's), after the release signature checks out. Programs still running from the old
/// folder keep working (it is moved aside, then deleted). Returns `false` when `dir`
/// already holds exactly that release.
pub async fn install_zip_release(base: &str, asset: &str, dir: &Path) -> Result<bool> {
    install_zip_release_with_key(base, asset, dir, &official_key()?).await
}

/// [`install_zip_release`] authenticated with `key` instead of yt-dlp's embedded key.
///
/// The app always calls [`install_zip_release`]. Tests pass a key they can sign with,
/// because they cannot produce a signature under the release key.
pub async fn install_zip_release_with_key(
    base: &str,
    asset: &str,
    dir: &Path,
    key: &SignedPublicKey,
) -> Result<bool> {
    install_release(base, asset, dir, InstallKind::Zip, key).await
}

/// Installs the single-file release asset `<base>/<asset>` as `file`. `asset` is the name
/// in the signed checksum file; `file`'s name may differ (`yt-dlp_linux` is saved as
/// `yt-dlp`). Returns `false` when `file` is already that release.
pub async fn install_file_release(base: &str, asset: &str, file: &Path) -> Result<bool> {
    install_file_release_with_key(base, asset, file, &official_key()?).await
}

/// [`install_file_release`] authenticated with `key`. See [`install_zip_release_with_key`].
pub async fn install_file_release_with_key(
    base: &str,
    asset: &str,
    file: &Path,
    key: &SignedPublicKey,
) -> Result<bool> {
    install_release(base, asset, file, InstallKind::File, key).await
}

async fn install_release(
    base: &str,
    asset: &str,
    dest: &Path,
    kind: InstallKind,
    key: &SignedPublicKey,
) -> Result<bool> {
    single_component(asset)?;
    let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) else {
        return Err(LinkError::Failed(format!("bad path {}", dest.display())));
    };
    let name = name.to_string_lossy();
    let pid = std::process::id();
    // Next to `dest`, so the final moves are renames on one file system.
    let work = parent.join(format!(".{name}-update-{pid}"));
    remove(&work);
    fs::create_dir_all(&work)?;

    let result = async {
        let sums_path = work.join(SUMS);
        let sig_path = work.join(SUMS_SIG);
        download_named(base, SUMS, &sums_path).await?;
        download_named(base, SUMS_SIG, &sig_path).await?;
        let sums = fs::read(&sums_path)?;
        let signature = fs::read(&sig_path)?;
        let expected = signed_asset_digest(key, &sums, &signature, asset)?;
        if already_installed(dest, kind, &expected) {
            return Ok(false);
        }

        let downloaded = work.join(asset);
        download_named(base, asset, &downloaded).await?;
        let hash = {
            let downloaded = downloaded.clone();
            tokio::task::spawn_blocking(move || sha256_of(&downloaded))
                .await
                .map_err(|err| LinkError::Failed(err.to_string()))??
        };
        require_digest(asset, &expected, &hash)?;

        let aside = parent.join(format!(".{name}-old-{pid}"));
        match kind {
            InstallKind::Zip => {
                let unpacked = work.join("unpacked");
                unzip(&downloaded, &unpacked).await?;
                fs::write(unpacked.join(MARKER), &expected)?;
                swap_in(&unpacked, dest, &aside)?;
            }
            InstallKind::File => {
                make_executable(&downloaded)?;
                swap_in(&downloaded, dest, &aside)?;
                fs::write(marker_path(dest, kind)?, &expected)?;
            }
        }
        Ok(true)
    }
    .await;
    remove(&work);
    result
}

#[cfg(test)]
mod tests {
    use pgp::types::KeyDetails;

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

    #[test]
    fn this_machine_has_a_known_ytdlp_release() {
        let release = host_release().unwrap();
        let expected = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => HostRelease::File {
                asset: "yt-dlp_linux",
            },
            ("linux", "aarch64") => HostRelease::File {
                asset: "yt-dlp_linux_aarch64",
            },
            ("macos", "aarch64" | "x86_64") => HostRelease::Zip {
                asset: "yt-dlp_macos.zip",
            },
            ("windows", "x86_64") => HostRelease::File {
                asset: "yt-dlp.exe",
            },
            (os, arch) => panic!("no expected yt-dlp asset for {os} {arch}"),
        };
        assert_eq!(release, expected);
    }

    /// `SHA2-256SUMS` and `SHA2-256SUMS.sig` from yt-dlp 2026.10.26 (signed 2026-10-26).
    /// The signature is the binary packet GitHub publishes, checked here against the
    /// embedded key so a parser or key mismatch fails in CI.
    #[test]
    fn the_official_key_accepts_a_real_ytdlp_signature_and_rejects_tampering() {
        let key = official_key().unwrap();
        assert_eq!(key.fingerprint().to_string(), YTDLP_RELEASE_FINGERPRINT);
        let sums = include_bytes!("../tests/fixtures/ytdlp-SHA2-256SUMS");
        let signature = include_bytes!("../tests/fixtures/ytdlp-SHA2-256SUMS.sig");

        let digest = signed_asset_digest(&key, sums, signature, "yt-dlp_linux").unwrap();
        assert_eq!(digest.len(), 64);

        let mut tampered = sums.to_vec();
        tampered[10] ^= 0x01;
        let error = verify_signed_checksums(&key, &tampered, signature).unwrap_err();
        assert!(
            error.to_string().contains("not signed by the release key"),
            "{error}"
        );

        let error = verify_signed_checksums(&key, sums, b"").unwrap_err();
        assert!(error.to_string().contains("missing"), "{error}");

        let error = verify_signed_checksums(&key, sums, b"not a signature").unwrap_err();
        assert!(error.to_string().contains("could not be read"), "{error}");
    }

    #[test]
    fn a_signed_checksum_rejects_a_tampered_archive_and_a_different_key() {
        let (secret, public) = test_keypair();
        let archive = b"yt-dlp bytes";
        let hash = sha256_hex(archive);
        let sums = format!("{hash}  yt-dlp_linux\n");
        let signature = sign(&secret, sums.as_bytes());

        assert_eq!(
            signed_asset_digest(&public, sums.as_bytes(), &signature, "yt-dlp_linux").unwrap(),
            hash
        );
        // The checksum line still describes `archive`; different bytes must not install.
        let error = require_digest("yt-dlp_linux", &hash, &sha256_hex(b"tampered")).unwrap_err();
        assert!(error.to_string().contains("signed checksum"), "{error}");

        // Extra byte after a hash line that still matches the archive. The signature is
        // over the original file, so this is the "replace the checksums" attack.
        let mut forged = sums.clone().into_bytes();
        forged.push(b'\n');
        let error = verify_signed_checksums(&public, &forged, &signature).unwrap_err();
        assert!(
            error.to_string().contains("not signed by the release key"),
            "{error}"
        );

        let error = verify_signed_checksums(&public, sums.as_bytes(), b"").unwrap_err();
        assert!(error.to_string().contains("missing"), "{error}");

        let (_other_secret, other_public) = test_keypair();
        let error =
            verify_signed_checksums(&other_public, sums.as_bytes(), &signature).unwrap_err();
        assert!(
            error.to_string().contains("not signed by the release key"),
            "{error}"
        );
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn test_keypair() -> (pgp::composed::SignedSecretKey, SignedPublicKey) {
        use pgp::composed::{KeyType, SecretKeyParamsBuilder};
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

    fn sign(secret: &pgp::composed::SignedSecretKey, data: &[u8]) -> Vec<u8> {
        use pgp::crypto::hash::HashAlgorithm;
        use pgp::ser::Serialize;
        use pgp::types::Password;
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
}

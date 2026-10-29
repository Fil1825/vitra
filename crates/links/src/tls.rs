//! Trust configuration for OpenSSL FFmpeg/ffprobe sidecars and their parent yt-dlp.
//! Only child environments change; explicit certificate settings always take precedence.

use std::ffi::OsString;
use std::io;
#[cfg(any(not(windows), test))]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

fn setting(command: &Command, name: &str) -> Option<OsString> {
    command
        .get_envs()
        .find(|(key, _)| *key == name)
        .map_or_else(
            || std::env::var_os(name),
            |(_, value)| value.map(Into::into),
        )
        .filter(|value| !value.is_empty())
}

#[cfg(any(not(windows), test))]
fn certificate_file(paths: &[&Path]) -> Option<PathBuf> {
    paths
        .iter()
        .find(|path| path.is_file())
        .map(|path| (*path).into())
}

/// Supply OS trust to a child and any FFmpeg/ffprobe processes it launches. Never disable
/// certificate verification or replace a caller's custom CA file/directory.
pub fn configure(command: &mut Command) -> io::Result<()> {
    if setting(command, "SSL_CERT_FILE").is_some() || setting(command, "SSL_CERT_DIR").is_some() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    let file = certificate_file(&[Path::new("/etc/ssl/cert.pem")]);
    #[cfg(not(any(target_os = "macos", windows)))]
    let file = certificate_file(&[
        Path::new("/etc/ssl/certs/ca-certificates.crt"),
        Path::new("/etc/pki/tls/certs/ca-bundle.crt"),
        Path::new("/etc/ssl/ca-bundle.pem"),
        Path::new("/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem"),
        Path::new("/etc/ssl/cert.pem"),
    ]);
    #[cfg(windows)]
    let file = Some(windows_roots()?);
    if let Some(file) = file {
        command.env("SSL_CERT_FILE", file);
    }
    Ok(())
}

#[cfg(windows)]
fn windows_roots() -> io::Result<PathBuf> {
    use std::io::Write;
    use std::sync::Mutex;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    // Keep the random, exclusively created file alive while child processes may use it.
    static ROOTS: Mutex<Option<tempfile::NamedTempFile>> = Mutex::new(None);
    let mut roots = ROOTS
        .lock()
        .map_err(|_| io::Error::other("CA cache lock poisoned"))?;
    if roots.is_none() {
        let native = rustls_native_certs::load_native_certs();
        if native.certs.is_empty() {
            return Err(io::Error::other(format!(
                "no Windows root certificates: {:?}",
                native.errors
            )));
        }
        let mut file = tempfile::Builder::new()
            .prefix("vitra-ca-")
            .suffix(".pem")
            .tempfile()?;
        for cert in native.certs {
            writeln!(file, "-----BEGIN CERTIFICATE-----")?;
            let encoded = STANDARD.encode(cert.as_ref());
            for line in encoded.as_bytes().chunks(64) {
                file.write_all(line)?;
                file.write_all(b"\n")?;
            }
            writeln!(file, "-----END CERTIFICATE-----")?;
        }
        file.flush()?;
        *roots = Some(file);
    }
    Ok(roots
        .as_ref()
        .expect("roots initialized")
        .path()
        .to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_trust_is_preserved_for_children() {
        for key in ["SSL_CERT_FILE", "SSL_CERT_DIR"] {
            let mut child = Command::new("ffmpeg");
            child.env(key, "/custom trust");
            configure(&mut child).unwrap();
            assert_eq!(setting(&child, key), Some("/custom trust".into()));
            assert_eq!(
                child.get_envs().count(),
                1,
                "do not add a file over an explicit CA directory"
            );
        }
    }

    #[test]
    fn certificate_candidates_must_be_files_and_follow_priority() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.pem");
        let second = dir.path().join("second.pem");
        std::fs::write(&second, "roots").unwrap();
        assert_eq!(
            certificate_file(&[dir.path(), &first, &second]),
            Some(second)
        );
        std::fs::write(&first, "roots").unwrap();
        assert_eq!(certificate_file(&[&first, dir.path()]), Some(first));
    }
}

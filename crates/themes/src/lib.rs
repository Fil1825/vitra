//! Vitra theme packages (`.vitratheme`): a zip of `theme.json`, an optional `custom.css` and
//! an `assets/` folder of pictures and fonts. Installed themes live unpacked in a folder of
//! their own (`<themes>/<id>/`).
//!
//! Packages come from strangers, so reading one trusts nothing: sizes are capped as they are
//! read (not as the archive claims), names must look like the files a theme can have (no
//! folders above, no links, nothing that could be a script), and anything else is left out.
//! What `theme.json` and `custom.css` *say* is checked by the frontend, which owns the
//! theme schema and the CSS rules (see docs/THEMES.md).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const THEME_FILE: &str = "theme.json";
pub const CSS_FILE: &str = "custom.css";
pub const ASSETS_DIR: &str = "assets";
/// The package's file extension.
pub const EXTENSION: &str = "vitratheme";

/// Pictures a theme may carry. No SVG: it can hold scripts.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];
/// Font files a theme may carry.
pub const FONT_EXTENSIONS: &[&str] = &["woff2", "woff", "ttf", "otf"];

/// How much a package may hold. Generous for a theme (a few pictures and fonts), small
/// enough that a hostile archive can't fill the disk or the memory.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The `.vitratheme` file itself.
    pub archive: u64,
    /// Everything unpacked together.
    pub unpacked: u64,
    /// Entries in the archive, ignored ones included.
    pub entries: usize,
    /// `theme.json` and `custom.css` each.
    pub text: u64,
    /// One picture or font.
    pub asset: u64,
    /// Pictures and fonts together.
    pub assets: usize,
}

pub const LIMITS: Limits = Limits {
    archive: 32 << 20,
    unpacked: 64 << 20,
    entries: 512,
    text: 512 << 10,
    asset: 16 << 20,
    assets: 64,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("not a zip archive: {0}")]
    Zip(String),
    #[error("the package has no theme.json")]
    NoTheme,
    #[error("{0} is too large")]
    TooLarge(String),
    #[error("{0} is not UTF-8 text")]
    NotText(&'static str),
    #[error("the package names a file {0:?}, which a theme can't have")]
    BadName(String),
    #[error("the package has {0} twice")]
    Duplicate(String),
    #[error("{0:?} is not a theme id")]
    BadId(String),
    #[error("{0} is not a picture or font a theme can use")]
    BadAsset(String),
}

impl From<zip::result::ZipError> for Error {
    fn from(err: zip::result::ZipError) -> Self {
        match err {
            zip::result::ZipError::Io(err) => Error::Io(err),
            other => Error::Zip(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A theme's files: `theme.json`, `custom.css` and the assets by file name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Package {
    pub theme: String,
    pub css: Option<String>,
    pub assets: BTreeMap<String, Vec<u8>>,
}

/// Theme ids are folder names: lowercase letters, digits and hyphens (as in the schema).
pub fn is_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// What an asset is, by its (lowercase) extension; `None` for anything a theme can't carry.
pub fn mime_type(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    Some(match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return None,
    })
}

/// An asset's file name as the schema allows it: a letter or digit, then letters, digits,
/// dots, underscores or hyphens (64 at most), and a known lowercase extension.
pub fn is_asset_name(name: &str) -> bool {
    let Some((stem, _)) = name.rsplit_once('.') else {
        return false;
    };
    mime_type(name).is_some()
        && name.len() <= 70
        && stem.len() <= 64
        && stem
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// What an entry of the archive is to a theme.
enum Entry {
    Theme,
    Css,
    Asset(String),
    /// Something else that may sit in a zip (a readme, macOS's `__MACOSX/`, `.DS_Store`).
    Ignored,
}

/// Classifies a path inside the package (below its root). Anything that could escape the
/// theme's folder is refused outright rather than ignored: no honest package has it.
fn entry(path: &str) -> Result<Entry> {
    let bad = || Error::BadName(path.to_owned());
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') || path.contains(':') {
        return Err(bad());
    }
    // `./theme.json` and doubled slashes are harmless; `..` never is.
    let parts: Vec<&str> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return Err(bad());
    }
    Ok(match parts.as_slice() {
        [THEME_FILE] => Entry::Theme,
        [CSS_FILE] => Entry::Css,
        [ASSETS_DIR, name] if is_asset_name(name) => Entry::Asset((*name).to_owned()),
        _ => Entry::Ignored,
    })
}

/// Reads a `.vitratheme` file.
pub fn read_package(path: &Path) -> Result<Package> {
    let file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > LIMITS.archive {
        return Err(Error::TooLarge(format!("the package ({} MB)", size >> 20)));
    }
    read_package_from(file)
}

/// Reads a package from any zip. Its files may sit in one folder (a zipped theme folder):
/// whatever folder `theme.json` is in is the package's root.
pub fn read_package_from<R: Read + Seek>(reader: R) -> Result<Package> {
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() > LIMITS.entries {
        return Err(Error::TooLarge("the package's list of files".into()));
    }
    let root = package_root(&mut archive)?;

    let mut package = Package::default();
    let mut theme = None;
    let mut unpacked = 0;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name()?.into_owned();
        let Some(path) = plain(&name).strip_prefix(root.as_str()) else {
            continue;
        };
        if file.is_symlink() {
            log::info!("theme package: leaving out the link {name}");
            continue;
        }
        let kind = entry(path)?;
        let limit = match kind {
            Entry::Theme | Entry::Css => LIMITS.text,
            Entry::Asset(_) => LIMITS.asset,
            Entry::Ignored => {
                log::info!("theme package: leaving out {name}");
                continue;
            }
        };
        let bytes = read_capped(&mut file, limit, path)?;
        unpacked += bytes.len() as u64;
        if unpacked > LIMITS.unpacked {
            return Err(Error::TooLarge("the unpacked package".into()));
        }
        match kind {
            Entry::Theme => {
                if theme.replace(text(bytes, THEME_FILE)?).is_some() {
                    return Err(Error::Duplicate(THEME_FILE.into()));
                }
            }
            Entry::Css => {
                if package.css.replace(text(bytes, CSS_FILE)?).is_some() {
                    return Err(Error::Duplicate(CSS_FILE.into()));
                }
            }
            Entry::Asset(asset) => {
                if package.assets.len() == LIMITS.assets {
                    return Err(Error::TooLarge("the list of assets".into()));
                }
                if package.assets.insert(asset.clone(), bytes).is_some() {
                    return Err(Error::Duplicate(asset));
                }
            }
            Entry::Ignored => {}
        }
    }
    package.theme = theme.ok_or(Error::NoTheme)?;
    Ok(package)
}

/// The folder `theme.json` is in (`""` or `"name/"`); the shallowest one wins.
fn package_root<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<String> {
    let mut root: Option<String> = None;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name()?;
        let Some(folder) = plain(&name).strip_suffix(THEME_FILE) else {
            continue;
        };
        let depth = folder.matches('/').count();
        let at_root = folder.is_empty();
        let one_folder = depth == 1 && folder.ends_with('/') && !folder.starts_with("__MACOSX");
        if (at_root || one_folder)
            && root
                .as_ref()
                .is_none_or(|known| known.matches('/').count() > depth)
        {
            root = Some(folder.to_owned());
        }
    }
    root.ok_or(Error::NoTheme)
}

/// A name without the `./` some zip tools put in front.
fn plain(name: &str) -> &str {
    name.trim_start_matches("./")
}

fn read_capped(reader: &mut impl Read, limit: u64, name: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::TooLarge(name.to_owned()));
    }
    Ok(bytes)
}

fn text(bytes: Vec<u8>, name: &'static str) -> Result<String> {
    let text = String::from_utf8(bytes).map_err(|_| Error::NotText(name))?;
    // A byte-order mark some editors write isn't part of the JSON or CSS.
    Ok(text
        .strip_prefix('\u{feff}')
        .map(str::to_owned)
        .unwrap_or(text))
}

/// Writes a package as a `.vitratheme` file (through a temporary file, so a failed export
/// never leaves half a package where the user asked for one).
pub fn write_package(path: &Path, package: &Package) -> Result<()> {
    let temporary = temporary_beside(path);
    let result = (|| -> Result<()> {
        let mut zip = ZipWriter::new(fs::File::create(&temporary)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        zip.start_file(THEME_FILE, options)?;
        zip.write_all(package.theme.as_bytes())?;
        if let Some(css) = &package.css {
            zip.start_file(CSS_FILE, options)?;
            zip.write_all(css.as_bytes())?;
        }
        for (name, bytes) in &package.assets {
            // Pictures and fonts are compressed already.
            let stored = options.compression_method(CompressionMethod::Stored);
            zip.start_file(format!("{ASSETS_DIR}/{name}"), stored)?;
            zip.write_all(bytes)?;
        }
        zip.finish()?.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// An installed theme's folder.
pub fn theme_dir(themes: &Path, id: &str) -> Result<PathBuf> {
    if !is_theme_id(id) {
        return Err(Error::BadId(id.to_owned()));
    }
    Ok(themes.join(id))
}

/// Installs a package as theme `id`, replacing a theme of that id. The new folder is written
/// beside the old one first, so a failure leaves the installed theme as it was.
pub fn install(themes: &Path, id: &str, package: &Package) -> Result<()> {
    let target = theme_dir(themes, id)?;
    fs::create_dir_all(themes)?;
    let staging = themes.join(format!(".{id}.{}", unique()));
    let result = (|| -> Result<()> {
        fs::create_dir_all(staging.join(ASSETS_DIR))?;
        fs::write(staging.join(THEME_FILE), &package.theme)?;
        if let Some(css) = &package.css {
            fs::write(staging.join(CSS_FILE), css)?;
        }
        for (name, bytes) in &package.assets {
            if !is_asset_name(name) {
                return Err(Error::BadAsset(name.clone()));
            }
            fs::write(staging.join(ASSETS_DIR).join(name), bytes)?;
        }
        let old = themes.join(format!(".{id}.old.{}", unique()));
        let had_one = target.exists();
        if had_one {
            fs::rename(&target, &old)?;
        }
        if let Err(err) = fs::rename(&staging, &target) {
            if had_one {
                let _ = fs::rename(&old, &target);
            }
            return Err(err.into());
        }
        if had_one {
            let _ = fs::remove_dir_all(&old);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Reads an installed theme back as a package (to export it).
pub fn read_installed(themes: &Path, id: &str) -> Result<Package> {
    read_folder(&theme_dir(themes, id)?)
}

/// Reads a theme folder (`theme.json`, `custom.css`, `assets/`) as a package: an installed
/// theme, or a theme's sources such as the examples in the repository's `themes/`. Assets
/// whose names a theme can't have are left out (see [`asset_names`]).
pub fn read_folder(dir: &Path) -> Result<Package> {
    let theme = text(read_file(&dir.join(THEME_FILE), LIMITS.text)?, THEME_FILE)?;
    let css = match read_file(&dir.join(CSS_FILE), LIMITS.text) {
        Ok(bytes) => Some(text(bytes, CSS_FILE)?),
        Err(Error::Io(err)) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    let mut assets = BTreeMap::new();
    for name in asset_names(dir) {
        let bytes = read_file(&dir.join(ASSETS_DIR).join(&name), LIMITS.asset)?;
        assets.insert(name, bytes);
    }
    Ok(Package { theme, css, assets })
}

fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    read_capped(
        &mut fs::File::open(path)?,
        limit,
        &path.display().to_string(),
    )
}

/// The assets an installed theme has, by name.
pub fn asset_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir.join(ASSETS_DIR)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_asset_name(name))
        .collect();
    names.sort();
    names
}

/// An installed theme as the frontend gets it: its text files and the names of its assets.
#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    pub id: String,
    pub theme: String,
    pub css: Option<String>,
    pub assets: Vec<String>,
}

/// Installed themes, by id. A folder that can't be read is skipped (and logged).
pub fn list(themes: &Path) -> Vec<Installed> {
    let Ok(entries) = fs::read_dir(themes) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|id| is_theme_id(id))
        .collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let dir = themes.join(&id);
            let read = || -> Result<Installed> {
                let theme = text(read_file(&dir.join(THEME_FILE), LIMITS.text)?, THEME_FILE)?;
                let css = read_file(&dir.join(CSS_FILE), LIMITS.text)
                    .ok()
                    .and_then(|bytes| text(bytes, CSS_FILE).ok());
                Ok(Installed {
                    assets: asset_names(&dir),
                    id: id.clone(),
                    theme,
                    css,
                })
            };
            read()
                .inspect_err(|err| log::warn!("cannot read the theme {id}: {err}"))
                .ok()
        })
        .collect()
}

/// Saves an installed theme's text files (the editor), creating its folder for a new theme.
/// `css: None` removes `custom.css`.
pub fn save(themes: &Path, id: &str, theme: &str, css: Option<&str>) -> Result<()> {
    let dir = theme_dir(themes, id)?;
    fs::create_dir_all(&dir)?;
    write_atomically(&dir.join(THEME_FILE), theme.as_bytes())?;
    match css {
        Some(css) => write_atomically(&dir.join(CSS_FILE), css.as_bytes())?,
        None => match fs::remove_file(dir.join(CSS_FILE)) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err.into()),
            _ => {}
        },
    }
    Ok(())
}

/// Copies a picture or font into a theme's assets; returns its path in the theme
/// (`assets/<name>`). The name comes from the file's, made safe and unique.
pub fn add_asset(themes: &Path, id: &str, source: &Path) -> Result<String> {
    let dir = theme_dir(themes, id)?.join(ASSETS_DIR);
    let original = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let (stem, extension) = original.rsplit_once('.').unwrap_or((original, ""));
    let extension = extension.to_ascii_lowercase();
    let mut stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .skip_while(|c| !c.is_ascii_alphanumeric())
        .take(48)
        .collect();
    if stem.is_empty() {
        stem = "asset".into();
    }
    let mut name = format!("{stem}.{extension}");
    if !is_asset_name(&name) {
        return Err(Error::BadAsset(original.to_owned()));
    }
    let bytes = read_file(source, LIMITS.asset)?;
    fs::create_dir_all(&dir)?;
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{stem}-{n}.{extension}");
        n += 1;
    }
    write_atomically(&dir.join(&name), &bytes)?;
    Ok(format!("{ASSETS_DIR}/{name}"))
}

/// Installs a copy of the installed theme `from` as `to` with `theme` as its `theme.json`
/// (its CSS and assets come along).
pub fn duplicate(themes: &Path, from: &str, to: &str, theme: &str) -> Result<()> {
    let mut package = read_installed(themes, from)?;
    package.theme = theme.to_owned();
    install(themes, to, &package)
}

/// Removes an installed theme.
pub fn remove(themes: &Path, id: &str) -> Result<()> {
    match fs::remove_dir_all(theme_dir(themes, id)?) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err.into()),
        _ => Ok(()),
    }
}

/// A file inside an installed theme that may be served to the window: only its assets.
pub fn asset_path(themes: &Path, id: &str, name: &str) -> Option<PathBuf> {
    (is_asset_name(name))
        .then(|| theme_dir(themes, id).ok())
        .flatten()
        .map(|dir| dir.join(ASSETS_DIR).join(name))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = temporary_beside(path);
    let result = fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(result?)
}

fn temporary_beside(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.part", unique()));
    path.with_file_name(name)
}

/// Unique within the process and between processes, for temporary names.
fn unique() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests;

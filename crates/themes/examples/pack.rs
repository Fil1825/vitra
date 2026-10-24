//! Packs theme folders into `.vitratheme` files, the way Vitra exports them:
//!
//! ```sh
//! cargo run -p themes --example pack -- themes/dunes themes/paper
//! cargo run -p themes --example pack -- --out dist/themes themes/*/
//! ```
//!
//! Each folder holds `theme.json`, an optional `custom.css` and `assets/`; anything else
//! (a readme, a font's license) stays out of the package. The files go to `--out`
//! (`target/themes` by default), named after their folders.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut out = PathBuf::from("target/themes");
    let mut folders = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => match args.next() {
                Some(dir) => out = PathBuf::from(dir),
                None => return usage(),
            },
            "-h" | "--help" => return usage(),
            _ => folders.push(PathBuf::from(arg)),
        }
    }
    if folders.is_empty() {
        return usage();
    }
    let mut failed = false;
    for folder in &folders {
        match pack(folder, &out) {
            Ok(file) => println!("{}", file.display()),
            Err(err) => {
                eprintln!("{}: {err}", folder.display());
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: pack [--out <folder>] <theme folder>...");
    ExitCode::from(2)
}

fn pack(folder: &Path, out: &Path) -> Result<PathBuf, String> {
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("not a folder name")?;
    let package = themes::read_folder(folder).map_err(|err| err.to_string())?;
    // An asset with a name a theme can't have would silently go missing from the package.
    if let Ok(entries) = fs::read_dir(folder.join(themes::ASSETS_DIR)) {
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !package.assets.contains_key(&file) {
                return Err(format!(
                    "assets/{file} can't go into a theme (see docs/THEMES.md)"
                ));
            }
        }
    }
    fs::create_dir_all(out).map_err(|err| err.to_string())?;
    let file = out.join(format!("{name}.{}", themes::EXTENSION));
    themes::write_package(&file, &package).map_err(|err| err.to_string())?;
    Ok(file)
}

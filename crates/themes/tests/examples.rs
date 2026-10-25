//! The example themes in the repository's `themes/` folder pack and read back like any
//! exported theme. (Their contents are checked against the theme schema on the frontend,
//! `src/theme/examples.test.ts`.)

use std::fs;
use std::path::{Path, PathBuf};

fn examples() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
    let mut folders: Vec<PathBuf> = fs::read_dir(root)
        .expect("the themes folder")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(themes::THEME_FILE).is_file())
        .collect();
    folders.sort();
    folders
}

#[test]
fn there_are_example_themes() {
    assert!(examples().len() >= 2, "{:?}", examples());
}

#[test]
fn every_example_packs_and_reads_back() {
    let tmp = tempfile::tempdir().unwrap();
    for folder in examples() {
        let package = themes::read_folder(&folder).unwrap();
        let assets: Vec<String> = fs::read_dir(folder.join(themes::ASSETS_DIR))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        for asset in &assets {
            assert!(
                package.assets.contains_key(asset),
                "{}: assets/{asset} would be left out",
                folder.display()
            );
        }

        let file = tmp.path().join("example.vitratheme");
        themes::write_package(&file, &package).unwrap();
        assert_eq!(themes::read_package(&file).unwrap(), package);
        assert!(
            fs::metadata(&file).unwrap().len() < 1 << 20,
            "{}: an example should stay small",
            folder.display()
        );
    }
}

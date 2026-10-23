use std::io::Cursor;

use super::*;

const THEME: &str = r#"{ "schemaVersion": 1, "id": "sunset", "name": "Sunset" }"#;

/// A zip with these entries (name, bytes); `None` makes a symlink to `/etc/passwd`.
fn zip(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, bytes) in entries {
        match bytes {
            Some(bytes) => {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            None => zip.add_symlink(*name, "/etc/passwd", options).unwrap(),
        }
    }
    zip.finish().unwrap().into_inner()
}

fn read(bytes: Vec<u8>) -> Result<Package> {
    read_package_from(Cursor::new(bytes))
}

fn sample() -> Package {
    Package {
        theme: THEME.into(),
        css: Some(":root { --x: 1px; }".into()),
        assets: BTreeMap::from([
            ("bg.jpg".to_owned(), vec![0xff, 0xd8, 0xff, 1, 2, 3]),
            ("Inter-Var.woff2".to_owned(), b"wOF2....".to_vec()),
        ]),
    }
}

#[test]
fn a_package_survives_writing_and_reading() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sunset.vitratheme");
    write_package(&path, &sample()).unwrap();
    assert_eq!(read_package(&path).unwrap(), sample());
    // Nothing temporary is left beside it.
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn a_zipped_theme_folder_reads_like_a_package() {
    let package = read(zip(&[
        ("sunset/", Some(b"")),
        ("sunset/theme.json", Some(THEME.as_bytes())),
        ("sunset/assets/bg.png", Some(b"png")),
        ("__MACOSX/sunset/._theme.json", Some(b"junk")),
        ("other/theme.json", Some(b"{}")),
    ]))
    .unwrap();
    assert_eq!(package.theme, THEME);
    assert_eq!(package.assets.keys().collect::<Vec<_>>(), ["bg.png"]);
}

#[test]
fn files_a_theme_cant_have_are_left_out() {
    let package = read(zip(&[
        ("./theme.json", Some("\u{feff}{}".as_bytes())),
        ("README.md", Some(b"hello")),
        (".DS_Store", Some(b"junk")),
        ("assets/logo.svg", Some(b"<svg onload=alert(1)>")),
        ("assets/run.js", Some(b"alert(1)")),
        ("assets/.hidden.png", Some(b"x")),
        ("assets/deep/pic.png", Some(b"x")),
        ("assets/link.png", None),
        ("assets/ok.webp", Some(b"x")),
    ]))
    .unwrap();
    assert_eq!(package.theme, "{}");
    assert_eq!(package.css, None);
    assert_eq!(package.assets.keys().collect::<Vec<_>>(), ["ok.webp"]);
}

#[test]
fn names_that_leave_the_folder_are_refused() {
    for name in [
        "../theme.json",
        "assets/../../evil.png",
        "/theme.json",
        "assets\\evil.png",
        "C:/evil.png",
    ] {
        let result = read(zip(&[("theme.json", Some(b"{}")), (name, Some(b"x"))]));
        assert!(
            matches!(result, Err(Error::BadName(_))),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn a_package_needs_its_theme() {
    assert!(matches!(
        read(zip(&[("custom.css", Some(b"a{}"))])),
        Err(Error::NoTheme)
    ));
    assert!(matches!(read(b"not a zip".to_vec()), Err(Error::Zip(_))));
    assert!(matches!(
        read(zip(&[("theme.json", Some(&[0xff, 0xfe, 0]))])),
        Err(Error::NotText(THEME_FILE))
    ));
}

#[test]
fn sizes_are_capped_as_they_unpack() {
    // Zeros compress to almost nothing: the archive is small, its content isn't.
    let big = vec![0_u8; (LIMITS.asset + 1) as usize];
    let archive = zip(&[("theme.json", Some(b"{}")), ("assets/big.png", Some(&big))]);
    assert!(archive.len() < 1 << 20);
    assert!(matches!(read(archive), Err(Error::TooLarge(_))));
    let long = vec![b' '; (LIMITS.text + 1) as usize];
    assert!(matches!(
        read(zip(&[("theme.json", Some(&long))])),
        Err(Error::TooLarge(_))
    ));
}

#[test]
fn installing_replaces_the_theme_of_that_id() {
    let tmp = tempfile::tempdir().unwrap();
    let themes = tmp.path().join("themes");
    install(&themes, "sunset", &sample()).unwrap();
    assert_eq!(read_installed(&themes, "sunset").unwrap(), sample());

    let plain = Package {
        theme: "{}".into(),
        ..Package::default()
    };
    install(&themes, "sunset", &plain).unwrap();
    assert_eq!(read_installed(&themes, "sunset").unwrap(), plain);
    // Only the theme's folder is left, no staging or old copies.
    let names: Vec<_> = fs::read_dir(&themes)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["sunset"]);

    assert!(matches!(
        install(&themes, "../up", &plain),
        Err(Error::BadId(_))
    ));
}

#[test]
fn installed_themes_are_listed_saved_and_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let themes = tmp.path();
    install(themes, "sunset", &sample()).unwrap();
    save(themes, "mono", "{\"id\":\"mono\"}", None).unwrap();
    fs::create_dir_all(themes.join("Not An Id")).unwrap();

    let listed = list(themes);
    assert_eq!(
        listed.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["mono", "sunset"]
    );
    assert_eq!(listed[1].assets, ["Inter-Var.woff2", "bg.jpg"]);
    assert_eq!(listed[1].css.as_deref(), Some(":root { --x: 1px; }"));

    save(themes, "sunset", THEME, None).unwrap();
    assert_eq!(list(themes)[1].css, None);
    remove(themes, "sunset").unwrap();
    remove(themes, "sunset").unwrap();
    assert_eq!(list(themes).len(), 1);
}

#[test]
fn assets_are_copied_in_under_safe_unique_names() {
    let tmp = tempfile::tempdir().unwrap();
    let themes = tmp.path().join("themes");
    save(&themes, "mono", "{}", None).unwrap();
    let picture = tmp.path().join("My Photo (1).JPG");
    fs::write(&picture, b"jpeg").unwrap();
    assert_eq!(
        add_asset(&themes, "mono", &picture).unwrap(),
        "assets/My-Photo--1-.jpg"
    );
    assert_eq!(
        add_asset(&themes, "mono", &picture).unwrap(),
        "assets/My-Photo--1--2.jpg"
    );
    let script = tmp.path().join("x.svg");
    fs::write(&script, b"<svg/>").unwrap();
    assert!(matches!(
        add_asset(&themes, "mono", &script),
        Err(Error::BadAsset(_))
    ));
}

#[test]
fn only_assets_are_served() {
    let themes = Path::new("/themes");
    assert_eq!(
        asset_path(themes, "mono", "bg.png"),
        Some(PathBuf::from("/themes/mono/assets/bg.png"))
    );
    assert_eq!(asset_path(themes, "mono", "../theme.json"), None);
    assert_eq!(asset_path(themes, "mono", "custom.css"), None);
    assert_eq!(asset_path(themes, "..", "bg.png"), None);
}

#[test]
fn asset_names_follow_the_schema() {
    for good in ["bg.png", "a.jpeg", "Inter-Var.woff2", "x_1.2.otf"] {
        assert!(is_asset_name(good), "{good}");
    }
    for bad in [
        "",
        ".png",
        "-a.png",
        "a.PNG",
        "a.svg",
        "a png.png",
        "a/b.png",
        "noext",
    ] {
        assert!(!is_asset_name(bad), "{bad}");
    }
}

fn main() {
    // Debug builds look for sidecars in `sidecars/<target triple>/` (see src/sidecar.rs).
    // Cargo sets TARGET for every build script; there is no recovery if it is missing.
    #[allow(clippy::expect_used)]
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=VITRA_TARGET_TRIPLE={target}");

    tauri_build::build();
}

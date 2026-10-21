fn main() {
    // Debug builds look for sidecars in `sidecars/<target triple>/` (see src/sidecar.rs).
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=VITRA_TARGET_TRIPLE={target}");

    tauri_build::build();
}

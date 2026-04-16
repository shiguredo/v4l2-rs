use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os != "linux" || target_arch != "aarch64" {
        return;
    }

    // sysroot のヘッダが無い環境 (ローカル macOS など) では何もしない
    let extra_args =
        env::var("BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu").unwrap_or_default();
    if !extra_args.contains("--sysroot=") {
        println!("cargo::warning=skip bindgen: sysroot not configured");
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let bindings = bindgen::Builder::default()
        .header_contents("wrapper.h", "#include <linux/videodev2.h>")
        .allowlist_type("v4l2_capability")
        .layout_tests(false)
        .derive_default(true)
        .generate()
        .expect("failed to generate videodev2 bindings");

    bindings
        .write_to_file(out_dir.join("videodev2.rs"))
        .expect("failed to write videodev2.rs");
}

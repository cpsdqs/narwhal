use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    #[cfg(target_os = "macos")]
    build_cocoa();
}

#[cfg(target_os = "macos")]
fn build_cocoa() {
    let mut objc_args = vec![
        // use objective-c
        "-xobjective-c",
        // use gnu11 language standard (Xcode seems to use it)
        "-std=gnu11",
        // target 10.7 APIs
        "-mmacosx-version-min=10.7",
        // use modules
        "-fmodules",
        // don’t generate an executable
        "-c",
    ];

    if env::var("PROFILE") == Ok("release".to_string()) {
        // enable optimizations
        objc_args.push("-O");
    }

    let mut objects = Vec::new();

    let proj_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    {
        let mut compile_objc = |out_dir: &str, name: &str| {
            let file_path = format!("{}/src/cocoa/{}.m", proj_dir, name);
            println!("cargo:rerun-if-changed={}", file_path);
            let status = Command::new("clang")
                .args(&objc_args)
                .arg(&file_path)
                .arg("-o")
                .arg(&format!("{}/{}.o", out_dir, name))
                .status()
                .unwrap();
            assert!(status.success(), "clang failed");
            objects.push(format!("{}.o", name));
        };

        compile_objc(&out_dir, "NCAppDelegate");
        compile_objc(&out_dir, "NCWindow");
    }

    let status = Command::new("ar")
        .current_dir(&Path::new(&out_dir))
        .args(&["crus", "libnarwhal_platform.a"])
        .args(&objects)
        .status()
        .unwrap();
    assert!(status.success(), "archive failed");

    println!("cargo:rustc-link-search={}", out_dir);
    println!("cargo:rustc-link-lib=static=narwhal_platform");

    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-search=framework={}/vulkan-sdk", proj_dir);
    println!("cargo:rustc-link-lib=framework=MoltenVK");
}

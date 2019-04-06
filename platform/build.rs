use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

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
        // target 10.11 APIs
        "-mmacosx-version-min=10.11",
        // use modules
        "-fmodules",
        // use ARC
        "-fobjc-arc",
        // generate debug info
        "-g",
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

            let output = Command::new("clang")
                .args(&objc_args)
                .arg(&file_path)
                .arg("-o")
                .arg(&format!("{}/{}.o", out_dir, name))
                .stderr(Stdio::piped())
                .output()
                .unwrap();

            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("{}", stderr);
            assert!(output.status.success(), "clang failed");

            // detect warnings by reading the last line which looks something like
            // “1 warning and 1 error generated.”
            if let Some(line) = String::from_utf8_lossy(&output.stderr).lines().last() {
                let mut parts = line.split_whitespace();
                let count = parts.next().unwrap_or("");
                let warning = parts.next().unwrap_or("");
                if warning.starts_with("warning")
                    && count.parse::<i64>().ok().map_or(false, |n| n > 0)
                {
                    panic!("clang generated warnings");
                }
            }

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

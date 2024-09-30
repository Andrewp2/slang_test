use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};
use walkdir::WalkDir;

fn main() {
    let out_dir = "out".to_string();
    let slang_include_path = "/home/andrew-peterson/Downloads/slang-2024.9.1-linux-x86_64/include";
    let slang_lib_path = "/home/andrew-peterson/Downloads/slang-2024.9.1-linux-x86_64/lib";
    cc::Build::new()
        .cpp(true)
        .file("shader_compiler.cpp")
        .include(slang_include_path)
        .include("/usr/include/nlohmann")
        .flag_if_supported("-std=c++17")
        .compile("shader_compiler");

    let exe_path = PathBuf::from(&out_dir).join("shader_compiler");
    // let mut perms = fs::metadata(&exe_path).unwrap().permissions();
    // perms.set_mode(0o755);
    // fs::set_permissions(&exe_path, perms).unwrap();

    // println!("cargo:rustc-link-search=native={}", slang_lib_path);
    // println!("cargo:rustc-link-lib=dylib=slang");
    // println!("cargo:rustc-link-lib=dylib=stdc++");
    // println!("cargo:rustc-link-arg=-Wl,-rpath,{}", slang_lib_path);

    // let status = Command::new(&exe_path)
    //     .env("OUT_DIR", &out_dir)
    //     .status()
    //     .expect("Failed to execute the shader compiler");

    // if !status.success() {
    //     panic!("Shader compiler exited with a non-zero status");
    // }
    // println!("cargo:rerun-if-changed=shader_compiler.cpp");
    // println!("cargo:rerun-if-changed=assets/shaders");

    // for entry in WalkDir::new("assets/shaders") {
    //     let entry = entry.unwrap();
    //     if entry.file_type().is_file() {
    //         println!("cargo:rerun-if-changed={}", entry.path().display());
    //     }
    // }
}

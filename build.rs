use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=solver/index.ts");
    println!("cargo:rerun-if-changed=solver/ejs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let bundle_path = out_dir.join("cipher.bundle.js");

    let status = Command::new("bun")
        .args([
            "run",
            "build.ts",
            bundle_path.to_str().expect("valid bundle path"),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=cipher bundle compiled to OUT_DIR");
        }
        _ => panic!("failed to build cipher bundle with bun"),
    }
}

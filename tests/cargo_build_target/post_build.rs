use std::{env, path::PathBuf};

fn main() {
    let current_dir = env::current_dir().unwrap();
    let current_parent = current_dir.parent().unwrap();
    assert!(env::vars()
        .position(|(k, v)| k == "CARGO_BUILD_TARGET" && v == "x86_64-unknown-none")
        .is_some());
    assert_eq!(
        env::var("CRATE_BUILD_COMMAND").unwrap(),
        "cargo build --package cargo_build_target"
    );
    assert_eq!(
        PathBuf::from(env::var("CRATE_MANIFEST_DIR").unwrap()),
        current_dir
    );
    assert_eq!(
        PathBuf::from(env::var("CRATE_MANIFEST_PATH").unwrap()),
        current_dir.join("Cargo.toml")
    );
    assert_eq!(env::var("CRATE_PROFILE").unwrap(), "debug");
    assert_eq!(env::var("CRATE_TARGET").unwrap(), "x86_64-unknown-none");
    assert_eq!(env::var("CRATE_TARGET_TRIPLE").unwrap(), "x86_64-unknown-none");
    assert_eq!(
        PathBuf::from(env::var("CRATE_TARGET_DIR").unwrap()),
        current_parent.join("target")
    );
    assert_eq!(
        PathBuf::from(env::var("CRATE_OUT_DIR").unwrap()),
        current_parent.join("target").join("x86_64-unknown-none").join("debug")
    );
    println!("ok");
}

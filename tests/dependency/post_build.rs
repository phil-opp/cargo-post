use std::{path::{Path, PathBuf}, env};

use example as _;

fn main() {
    assert_eq!(env::var("CRATE_BUILD_COMMAND").unwrap(), "cargo build --package dependency");
    assert_eq!(PathBuf::from(env::var("CRATE_MANIFEST_DIR").unwrap()), Path::new(".").canonicalize().unwrap());
    assert_eq!(PathBuf::from(env::var("CRATE_MANIFEST_PATH").unwrap()), Path::new("Cargo.toml").canonicalize().unwrap());
    assert_eq!(env::var("CRATE_PROFILE").unwrap(), "debug");
    assert_eq!(env::var("CRATE_TARGET").unwrap(), "");
    assert_eq!(env::var("CRATE_TARGET_TRIPLE").unwrap(), "");
    assert_eq!(PathBuf::from(env::var("CRATE_TARGET_DIR").unwrap()), Path::new("../target").canonicalize().unwrap());
    assert_eq!(PathBuf::from(env::var("CRATE_OUT_DIR").unwrap()), Path::new("../target").join("debug").canonicalize().unwrap());
    println!("ok");
}

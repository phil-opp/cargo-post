use std::{env, path::PathBuf};

use example as _;

fn main() {
    let current_dir = env::current_dir().unwrap();

    let package_dir = if env::var("CRATE_BUILD_COMMAND")
        .unwrap()
        .contains("--package")
    {
        assert_eq!(
            env::var("CRATE_BUILD_COMMAND").unwrap(),
            "cargo build --package dependency"
        );
        current_dir
    } else {
        assert_eq!(env::var("CRATE_BUILD_COMMAND").unwrap(), "cargo build");
        current_dir.join("dependency")
    };

    let package_dir_parent = package_dir.parent().unwrap();

    assert_eq!(
        PathBuf::from(env::var("CRATE_MANIFEST_DIR").unwrap()),
        package_dir
    );
    assert_eq!(
        PathBuf::from(env::var("CRATE_MANIFEST_PATH").unwrap()),
        package_dir.join("Cargo.toml")
    );
    assert_eq!(
        PathBuf::from(env::var("CRATE_TARGET_DIR").unwrap()),
        package_dir_parent.join("target")
    );
    assert_eq!(
        PathBuf::from(env::var("CRATE_OUT_DIR").unwrap()),
        package_dir_parent.join("target").join("debug")
    );
    assert_eq!(env::var("CRATE_PROFILE").unwrap(), "debug");
    assert_eq!(env::var("CRATE_TARGET").unwrap(), "");
    assert_eq!(env::var("CRATE_TARGET_TRIPLE").unwrap(), "");
    println!("ok");
}

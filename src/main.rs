use std::{
    env,
    fs::{self, File},
    io::Read,
    ops::Deref,
    path::{Path, PathBuf},
    process::{self, Command},
};

static HELP: &str = include_str!("help.txt");

/// The required post_build script call
enum BuildScriptCall {
    /// No call the post build script is needed
    ///
    /// For example for `cargo check` or `cargo new`.
    NoCall,
    /// The build script needs to be run after executing the cargo command
    ///
    /// For example for `cargo build`.
    AfterCommand,
    // TODO: Special variants for e.g. `cargo run` where the post build script needs to be
    // run in between (i.e. after the build, but before running it).
}

fn main() {
    // check arguments
    let mut args = env::args().peekable();
    assert!(args.next().is_some(), "no executable name in args");
    if args.next().as_deref() != Some("post") {
        panic!("cargo-post must be invoked as `cargo post`");
    }
    if args.peek().map(Deref::deref) == Some("--help") {
        println!("{}", HELP);
        return;
    }
    if args.peek().map(Deref::deref) == Some("--version") {
        println!("cargo-post {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let build_script_call = match args.peek().map(Deref::deref) {
        Some(cmd) => match cmd {
            "b" | "build" | "xbuild" => BuildScriptCall::AfterCommand,
            "c" | "check" | "clean" | "doc" | "new" | "init" | "update" | "search"
            | "uninstall" => BuildScriptCall::NoCall,
            cmd if ["run", "test", "bench", "publish", "install"].contains(&cmd) => {
                panic!("`cargo post {}` is not supported yet", cmd)
            }
            cmd => panic!("unknown cargo command `cargo {}`", cmd),
        },
        None => BuildScriptCall::NoCall,
    };

    // run cargo
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    let exit_status = match cmd.status() {
        Ok(status) => status,
        Err(err) => panic!("failed to execute command `{:?}`: {:?}", cmd, err),
    };
    if !exit_status.success() {
        process::exit(exit_status.code().unwrap_or(1));
    }

    match build_script_call {
        BuildScriptCall::NoCall => {}
        BuildScriptCall::AfterCommand => {
            if let Some(exit_status) = run_post_build_script() {
                if !exit_status.success() {
                    process::exit(exit_status.code().unwrap_or(1));
                }
            }
        }
    };
}

fn run_post_build_script() -> Option<process::ExitStatus> {
    let rustc_metadata =
        rustc_version::version_meta().expect("cannot query rustc version metadata");

    let mut cmd = cargo_metadata::MetadataCommand::new();
    cmd.no_deps();
    let manifest_path = {
        let mut args = env::args().skip_while(|val| !val.starts_with("--manifest-path"));
        match args.next() {
            Some(ref p) if p == "--manifest-path" => Some(args.next().unwrap()),
            Some(p) => Some(p.trim_start_matches("--manifest-path=").to_owned()),
            None => None,
        }
    };
    if let Some(ref manifest_path) = manifest_path {
        cmd.manifest_path(manifest_path);
    }
    let metadata = cmd.exec().unwrap();

    let package = {
        let mut args =
            env::args().skip_while(|val| !val.starts_with("--package") && !val.starts_with("-p"));
        let package_name = match args.next() {
            Some(ref p) if p == "--package" || p == "-p" => Some(args.next().unwrap()),
            Some(p) => Some(p.trim_start_matches("--package=").to_owned()),
            None => None,
        };
        let mut packages = metadata.packages.iter();
        match package_name {
            Some(name) => packages
                .find(|p| p.name == name)
                .expect("specified package not found"),
            None => {
                let package = packages.next().expect("workspace has no packages");
                assert!(
                    packages.next().is_none(),
                    "Please specify a `--package` argument"
                );
                package
            }
        }
    };

    let manifest_path = manifest_path
        .map(PathBuf::from)
        .unwrap_or_else(|| package.manifest_path.clone().into());
    let manifest_dir = manifest_path.parent().expect("failed to get crate folder");
    let post_build_script_path = manifest_dir.join("post_build.rs");

    if !post_build_script_path.exists() {
        return None;
    }
    eprintln!(
        "Running Post Build Script at {}",
        post_build_script_path.display()
    );

    let cargo_toml: toml::Table = {
        let mut content = String::new();
        File::open(&manifest_path)
            .expect("Failed to open Cargo.toml")
            .read_to_string(&mut content)
            .expect("Failed to read Cargo.toml");
        content
            .parse::<toml::Table>()
            .expect("Failed to parse Cargo.toml")
    };

    let cargo_post_metadata = cargo_toml
        .get("package")
        .and_then(|table| table.get("metadata"))
        .and_then(|table| table.get("cargo-post"));

    let dependencies = cargo_post_metadata
        .and_then(|table| table.get("dependencies"))
        .cloned();
    let dependencies_string = if let Some(mut dependencies) = dependencies {
        // adjust path dependencies
        for (dep_name, dependency) in dependencies
            .as_table_mut()
            .unwrap_or(&mut toml::map::Map::new())
            .iter_mut()
        {
            if let Some(path) = dependency.get_mut("path") {
                let dep_path = manifest_dir.join(Path::new(
                    path.as_str().expect("dependency path not a string"),
                ));
                let path_canoncicalized = dep_path.canonicalize().unwrap_or_else(|_| {
                    panic!(
                        "Dependency {} does not exist at {}",
                        dep_name,
                        dep_path.display()
                    )
                });
                *path = toml::Value::String(
                    path_canoncicalized
                        .into_os_string()
                        .into_string()
                        .expect("dependency path is not valid UTF-8"),
                );
            }
        }

        let mut dependency_section = toml::value::Table::new();
        dependency_section.insert("dependencies".into(), dependencies);
        toml::to_string(&dependency_section)
            .expect("invalid toml in package.metadata.cargo-post.dependencies")
    } else {
        String::new()
    };

    // Create a dummy Cargo.toml for post build script
    let build_script_manifest_dir = metadata
        .target_directory
        .canonicalize()
        .expect("target directory does not exist")
        .join("post_build_script_manifest");
    fs::create_dir_all(&build_script_manifest_dir)
        .expect("failed to create build script manifest dir");
    let build_script_manifest_path = build_script_manifest_dir.join("Cargo.toml");
    let build_script_manifest_content = format!(
        include_str!("post_build_script_manifest.toml"),
        file_name = toml::Value::String(post_build_script_path.display().to_string()),
        dependencies = dependencies_string,
    );
    fs::write(&build_script_manifest_path, build_script_manifest_content)
        .expect("Failed to write post build script manifest");

    // gather arguments for post build script
    let target_path = {
        // Target resolution chooses the first available out of the following:
        // - target CLI flag
        // - $CARGO_BUILD_TARGET
        // - build.target in a .cargo/config file
        let mut args = env::args().skip_while(|val| !val.starts_with("--target"));
        match args.next() {
            Some(ref p) if p == "--target" => Some(args.next().expect("no target after --target")),
            Some(p) => Some(p.trim_start_matches("--target=").to_owned()),
            None => env::var("CARGO_BUILD_TARGET")
                .ok()
                .or(find_cargo_config_target(manifest_dir)),
        }
    };
    let target_triple = {
        let file_stem = target_path.as_ref().map(|t| {
            Path::new(t)
                .file_stem()
                .expect("target has no file stem")
                .to_owned()
        });
        file_stem.map(|s| s.into_string().expect("target not a valid string"))
    };
    let profile = if env::args().any(|arg| arg == "--release" || arg == "-r") {
        "release"
    } else {
        "debug"
    };
    let mut out_dir = metadata.target_directory.clone();
    if let Some(ref target_triple) = target_triple {
        out_dir.push(target_triple);
    }
    out_dir.push(profile);
    let build_command = {
        let mut cmd = String::from("cargo ");
        let args: Vec<String> = env::args().skip(2).collect();
        cmd.push_str(&args.join(" "));
        cmd
    };

    let is_target_mismatch = target_triple
        .as_ref()
        .is_some_and(|t| *t != rustc_metadata.host);

    // build post build script
    let mut cmd = Command::new("cargo");
    // run build command from home directory to avoid effects of `.cargo/config` files
    cmd.current_dir(home::cargo_home().unwrap());
    cmd.arg("build");
    cmd.arg("--manifest-path");
    cmd.arg(build_script_manifest_path.as_os_str());
    if is_target_mismatch {
        cmd.arg("--target");
        cmd.arg(&rustc_metadata.host);
    }
    let exit_status = cmd.status().expect("Failed to run post build script");
    if !exit_status.success() {
        process::exit(exit_status.code().unwrap_or(1));
    }

    // run post build script
    let cmd_path = {
        let mut path = build_script_manifest_dir.join("target");
        if is_target_mismatch {
            path.push(&rustc_metadata.host);
        }
        path.push("debug");
        path.push("post-build-script");
        path
    };
    let mut cmd = Command::new(cmd_path);
    cmd.env("CRATE_MANIFEST_DIR", manifest_dir.as_os_str());
    cmd.env(
        "CRATE_MANIFEST_PATH",
        manifest_dir.join("Cargo.toml").as_os_str(),
    );
    cmd.env("CRATE_TARGET_DIR", metadata.target_directory.as_os_str());
    cmd.env("CRATE_OUT_DIR", out_dir);
    cmd.env("CRATE_TARGET", target_path.unwrap_or_default());
    cmd.env("CRATE_TARGET_TRIPLE", target_triple.unwrap_or_default());
    cmd.env("CRATE_PROFILE", profile);
    cmd.env("CRATE_BUILD_COMMAND", build_command);
    Some(cmd.status().expect("Failed to run post build script"))
}

fn find_cargo_config_target(path: &Path) -> Option<String> {
    // Cargo config path resolution works in accordance with:
    // https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure

    // Set up a path for $CARGO_HOME
    let cargo_home = env::var("CARGO_HOME").unwrap();
    let cargo_home = Path::new(&cargo_home);
    // Depending on the path we enter this function with,
    // allocate a list of paths to check in order
    let paths = if path.eq(cargo_home) {
        vec![path.join("config.toml")]
    } else {
        vec![path.join(".cargo/config"), path.join(".cargo/config.toml")]
    };
    // First attempt to find and parse variants for current given path
    for config_path in paths {
        if config_path.exists() {
            let target = parse_build_target(&config_path);
            if target.is_some() {
                return target;
            }
        }
    }
    // We haven't found any config for $CARGO_HOME/config.toml;
    // stop recursing
    if path.eq(cargo_home) {
        return None;
    }

    if let Some(p) = path.parent() {
        // Our current path still has a parent, recurse into it
        find_cargo_config_target(p)
    } else {
        if path.ne(cargo_home) {
            // Our current path is effectively at the root of the volume;
            // attempt to find configuration at $CARGO_HOME/config.toml
            return find_cargo_config_target(cargo_home);
        }
        // All stop conditions have been met and no target has been found
        None
    }
}

fn parse_build_target(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).expect("cannot read cargo config file");
    let parsed: toml::Table = content.parse().expect("cannot parse cargo config toml");
    if let Some(build) = parsed.get("build") {
        if let Some(target) = build.get("target") {
            return Some(
                target
                    .as_str()
                    .expect("build.target should be a string")
                    .to_string(),
            );
        }
    }
    None
}

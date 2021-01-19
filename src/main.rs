use std::{
    collections::HashMap,
    env,
    ffi::OsString,
    fs::{self, File},
    io::Read,
    ops::Deref,
    path::{Path, PathBuf},
    process::{self, Command},
};

use cargo_metadata::{Metadata, Package};

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
    /// Special variants for e.g. `cargo run` where the post build script needs to be
    /// run in between (i.e. after the build, but before running it).
    InbetweenCommand,
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
            cmd if ["r", "t", "run", "test", "bench", "publish", "install"].contains(&cmd) => {
                BuildScriptCall::InbetweenCommand
            }
            cmd => panic!("unknown cargo command `cargo {}`", cmd),
        },
        None => BuildScriptCall::NoCall,
    };

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
        cmd.manifest_path(&manifest_path);
    }
    let metadata = cmd.exec().unwrap();

    let packages = {
        let mut args =
            env::args().skip_while(|val| !val.starts_with("--package") && !val.starts_with("-p"));
        let package_name = match args.next() {
            Some(ref p) if p == "--package" || p == "-p" => Some(args.next().unwrap()),
            Some(p) => Some(p.trim_start_matches("--package=").to_owned()),
            None => None,
        };

        let packages: Vec<_> = metadata
            .packages
            .iter()
            .filter(|&p| match package_name {
                Some(ref name) if &p.name == name => true,
                None => true,
                _ => false,
            })
            .collect();

        if package_name.is_some() && packages.is_empty() {
            panic!("specified package not found");
        }

        packages
    };

    let args: Vec<_> = args.collect();
    for package in packages {
        let mut package_args = args.clone();

        // run cargo build
        let exec_args: Vec<_> = if matches!(build_script_call, BuildScriptCall::InbetweenCommand) {
            package_args[0] = "build".to_owned();

            // Extract all executable args
            let mut args_iter = package_args.splitn(2, |val| val.as_str().eq("--"));
            let args = args_iter.next().unwrap_or_default().to_vec();
            let exec_args = args_iter.next().unwrap_or_default().to_vec();

            package_args = args;
            exec_args
        } else {
            Vec::new()
        };

        let mut cmd = Command::new("cargo");
        cmd.args(package_args);

        cmd.current_dir(package.manifest_path.parent().unwrap());

        match cmd.status() {
            Ok(status) if !status.success() => process::exit(status.code().unwrap_or(1)),
            Err(err) => panic!("failed to execute command `{:?}`: {:?}", cmd, err),
            _ => {}
        };

        if matches!(
            build_script_call,
            BuildScriptCall::AfterCommand | BuildScriptCall::InbetweenCommand
        ) {
            let (output, env_vars) =
                run_post_build_script(&metadata, manifest_path.as_ref(), package);

            if let Some(ref output) = output {
                if !output.status.success() {
                    process::exit(output.status.code().unwrap_or(1));
                }
            }

            let out = PathBuf::from(env_vars.get("CRATE_OUT_DIR").unwrap());
            let mut bins = env_vars
                .get("CRATE_OUT_BINS")
                .unwrap()
                .to_str()
                .unwrap()
                .split(':')
                .map(|b| out.join(b))
                .filter(|b| b.exists());

            let mut bin = bins.next().expect("Found no binary to be executed!");

            if bins.next().is_some() {
                panic!("More than one binary found! Use the `--bin` option to specify a binary, or the `default-run` manifest key")
            }

            // Parse stdout for `cargo:`
            if let Some(ref output) = output {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some(kv) = line.strip_prefix("cargo:updated-bin=") {
                        let mut kv_iter = kv.split('=');
                        match kv_iter.next() {
                            Some(k) if Some(k) == bin.to_str() => {
                                let v = PathBuf::from(kv_iter.next().expect("Missing new value in println!(\"cargo:updated-bin\") statement"));
                                if !v.exists() {
                                    panic!("New binary does not exist!");
                                }
                                bin = v;
                            }
                            Some(_k) => {
                                panic!(
                                    "Unknown binary in println!(\"cargo:updated-bin\") statement"
                                );
                            }
                            None => {
                                panic!("Malformed println!(\"cargo:updated-bin\") statement");
                            }
                        }
                    } else {
                        // Log everything else to allow debug logging
                        println!("{}", line);
                    }
                }
            }

            if matches!(build_script_call, BuildScriptCall::InbetweenCommand) {
                // Execute the resulting binary with the executable args passed in!
                let mut cmd = Command::new(bin);
                cmd.args(exec_args);

                match cmd.status() {
                    Ok(status) if !status.success() => process::exit(status.code().unwrap_or(1)),
                    Err(err) => panic!("failed to execute command `{:?}`: {:?}", cmd, err),
                    _ => {}
                };
            }
        };
    }
}

fn build_envs(
    metadata: &Metadata,
    package: &Package,
    manifest_dir: &Path,
) -> HashMap<String, OsString> {
    // gather arguments for post build script
    let target_path = {
        let mut args = env::args().skip_while(|val| !val.starts_with("--target"));
        match args.next() {
            Some(ref p) if p == "--target" => Some(args.next().expect("no target after --target")),
            Some(p) => Some(p.trim_start_matches("--target=").to_owned()),
            None => None,
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
    let profile = if env::args().any(|arg| arg == "--release") {
        "release"
    } else {
        "debug"
    };

    let mut out_dir = metadata.target_directory.clone();
    if let Some(ref target_triple) = target_triple {
        out_dir.push(target_triple);
    }
    out_dir.push(&profile);
    let build_command = {
        let mut cmd = String::from("cargo ");
        let args: Vec<String> = env::args().skip(2).collect();
        cmd.push_str(&args.join(" "));
        cmd
    };

    let all_examples = env::args().any(|arg| arg == "--examples");
    let example_name = {
        if all_examples {
            None
        } else {
            let mut args = env::args().skip_while(|val| !val.starts_with("--example"));
            match args.next() {
                Some(ref p) if p == "--example" => {
                    Some(args.next().expect("no example after --example"))
                }
                Some(p) => Some(p.trim_start_matches("--example=").to_owned()),
                None => None,
            }
        }
    };

    let bins: Vec<_> = package
        .targets
        .iter()
        .filter(|t| t.crate_types.contains(&"bin".to_owned()))
        .filter_map(|t| match example_name {
            Some(ref example) if t.kind.contains(&"example".to_owned()) && &t.name == example => {
                Path::new("examples")
                    .join(&t.name)
                    .to_owned()
                    .into_os_string()
                    .into_string()
                    .ok()
            }
            None if all_examples && t.kind.contains(&"example".to_owned()) => Path::new("examples")
                .join(&t.name)
                .to_owned()
                .into_os_string()
                .into_string()
                .ok(),
            None if !all_examples && t.kind.contains(&"bin".to_owned()) => Some(t.name.to_owned()),
            _ => None,
        })
        .collect();

    let mut post_env_vars: HashMap<String, OsString> = HashMap::new();

    post_env_vars.insert(
        "CRATE_MANIFEST_DIR".to_owned(),
        manifest_dir.to_owned().into_os_string(),
    );
    post_env_vars.insert(
        "CRATE_MANIFEST_PATH".to_owned(),
        manifest_dir.join("Cargo.toml").into_os_string(),
    );
    post_env_vars.insert(
        "CRATE_TARGET_DIR".to_owned(),
        metadata.target_directory.to_owned().into_os_string(),
    );
    post_env_vars.insert("CRATE_OUT_DIR".to_owned(), out_dir.into_os_string());
    post_env_vars.insert(
        "CRATE_TARGET".to_owned(),
        target_path.map(OsString::from).unwrap_or_default(),
    );
    post_env_vars.insert(
        "CRATE_TARGET_TRIPLE".to_owned(),
        OsString::from(target_triple.unwrap_or_default()),
    );
    post_env_vars.insert("CRATE_PROFILE".to_owned(), OsString::from(profile));
    post_env_vars.insert(
        "CRATE_BUILD_COMMAND".to_owned(),
        OsString::from(build_command),
    );
    post_env_vars.insert("CRATE_OUT_BINS".to_owned(), OsString::from(bins.join(":")));

    post_env_vars
}

fn run_post_build_script(
    metadata: &Metadata,
    manifest_path: Option<&String>,
    package: &Package,
) -> (Option<process::Output>, HashMap<String, OsString>) {
    let manifest_path = manifest_path
        .map(PathBuf::from)
        .unwrap_or_else(|| package.manifest_path.clone());
    let manifest_dir = manifest_path.parent().expect("failed to get crate folder");
    let post_build_script_path = manifest_dir.join("post_build.rs");

    let post_env_vars = build_envs(metadata, package, manifest_dir);

    if !post_build_script_path.exists() {
        return (None, post_env_vars);
    }
    println!(
        "Running Post Build Script at {}",
        post_build_script_path.display()
    );

    let cargo_toml: toml::Value = {
        let mut content = String::new();
        File::open(&manifest_path)
            .expect("Failed to open Cargo.toml")
            .read_to_string(&mut content)
            .expect("Failed to read Cargo.toml");
        content
            .parse::<toml::Value>()
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
            .unwrap_or(&mut toml::value::Map::new())
            .iter_mut()
        {
            if let Some(path) = dependency.get_mut("path") {
                let dep_path = Path::new(path.as_str().expect("dependency path not a string"));
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
        .clone()
        .join("post_build_script_manifest");
    fs::create_dir_all(&build_script_manifest_dir)
        .expect("failed to create build script manifest dir");
    let build_script_manifest_path = build_script_manifest_dir.join("Cargo.toml");
    let build_script_manifest_content = format!(
        include_str!("post_build_script_manifest.toml"),
        file_name = toml::to_string(&post_build_script_path.to_str())
            .expect("Failed to serialize post build script path as TOML string"),
        dependencies = dependencies_string,
    );
    fs::write(&build_script_manifest_path, build_script_manifest_content)
        .expect("Failed to write post build script manifest");

    // run post build script
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    cmd.arg("--manifest-path");
    cmd.arg(build_script_manifest_path.as_os_str());

    for (k, v) in &post_env_vars {
        cmd.env(k, v);
    }

    (
        Some(cmd.output().expect("Failed to run post build script")),
        post_env_vars,
    )
}

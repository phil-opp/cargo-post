# cargo-post

A `cargo` wrapper that executes a post build script after a successful build.

## Installation

```
cargo install cargo-post
```

## Usage

Execute `cargo CMD [ARGS]` and run `post_build.rs` afterwards:

```
cargo post CMD [ARGS]
```

The `post_build.rs` is only run if `CMD` is a build command like `build` or [`xbuild`](http://github.com/rust-osdev/cargo-xbuild/).

In workspaces, you might have to pass a `--package` argument to `cargo build` to specify the package for which the post build script should be run.

### Examples:

Build the crate and run `post_build.rs` afterwards:

```
cargo post build
```

Build the crate in release mode and run `post_build.rs` afterwards:

```
cargo post build --release
```

Builds the crate using [cargo-xbuild](http://github.com/rust-osdev/cargo-xbuild/):

```
cargo post xbuild
```

Check the crate without executing post_build:

```
cargo post check
```

The build script is not executed because `cargo check` is not a build command. The same behavior occurs for `cargo post doc` or `cargo post update`.

## Post-Build Script Format

Post-build scripts are similar to cargo build scripts, but they get a different set of environment variables:

- `CRATE_BUILD_COMMAND`: The full cargo command that was used for building without `post`
    - Example: When the crate is compiled using `cargo post build --release`, the environment variable has the value `cargo build --release`.
- `CRATE_MANIFEST_DIR`: The directory where the `Cargo.toml` of the crates lives.
- `CRATE_MANIFEST_PATH`: The path to the `Cargo.toml` of the crates.
- `CRATE_PROFILE`: `debug` or `release`, depending on whether `--release` was passed in the build command.
- `CRATE_TARGET`: The full content of what was passed as `--target` or the empty string if no `--target` was passed.
- `CRATE_TARGET_TRIPLE`: The target triple of the passed `--target` or the empty string if no `--target` was passed.
    - Example: With `cargo post xbuild --target /some/path/to/your/target-x86_64.json` this environment variable has the value `target-x86_64`.
- `CRATE_TARGET_DIR`: The path to the `target` directory of your crate.
- `CRATE_OUT_DIR`: The path to the directory where cargo puts the compiled binaries. This path is constructed by appending `CRATE_TARGET_TRIPLE` and `CRATE_PROFILE` to `CRATE_TARGET_DIR`.

## Dependencies

Dependencies for post build scripts can be specified in a `[package.metadata.cargo-post.dependencies]` table in your `Cargo.toml`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

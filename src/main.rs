#![forbid(unsafe_code)]
#![warn(future_incompatible, rust_2018_idioms, single_use_lifetimes, unreachable_pub)]
#![warn(clippy::default_trait_access, clippy::wildcard_imports)]

// Refs:
// - https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html

mod fs;
mod process;

use std::{env, ffi::OsString, path::Path};

use anyhow::Result;
use serde::Deserialize;
use structopt::{clap::AppSettings, StructOpt};

use crate::process::ProcessBuilder;

#[derive(StructOpt)]
#[structopt(
    bin_name = "cargo",
    rename_all = "kebab-case",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
enum Opts {
    /// A wrapper for source based code coverage (-Zinstrument-coverage).
    LlvmCov(Args),
}

#[derive(StructOpt)]
#[structopt(
    rename_all = "kebab-case",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
struct Args {
    #[structopt(long)]
    json: bool,
    #[structopt(long, conflicts_with = "json")]
    text: bool,
    #[structopt(long, conflicts_with_all = &["json", "text"])]
    html: bool,
    #[structopt(long, conflicts_with_all = &["json", "text"])]
    open: bool,

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/source-based-code-coverage.html#including-doc-tests
    /// Including doc tests (unstable)
    #[structopt(long)]
    doctests: bool,

    // FIXME: --package doesn't work properly, use --manifest-path instead for now.
    // /// Package to run tests for
    // #[structopt(short, long, value_name = "SPEC")]
    // package: Vec<String>,
    /// Test all packages in the workspace
    #[structopt(long, visible_alias = "all")]
    workspace: bool,
    /// Exclude packages from the test
    #[structopt(long, value_name = "SPEC")]
    exclude: Vec<String>,
    /// Build artifacts in release mode, with optimizations
    #[structopt(long)]
    release: bool,
    /// Space or comma separated list of features to activate
    #[structopt(long, value_name = "FEATURES")]
    features: Vec<String>,
    /// Activate all available features
    #[structopt(long)]
    all_features: bool,
    /// Do not activate the `default` feature
    #[structopt(long)]
    no_default_features: bool,
    /// Build for the target triple
    #[structopt(long, value_name = "TRIPLE")]
    target: Option<String>,
    /// Path to Cargo.toml
    #[structopt(long, value_name = "PATH")]
    manifest_path: Option<String>,

    /// Arguments for the test binary
    #[structopt(last = true, parse(from_os_str))]
    args: Vec<OsString>,
}

fn main() -> Result<()> {
    let Opts::LlvmCov(mut args) = Opts::from_args();
    args.html |= args.open;

    let metadata = metadata(None)?;
    fs::create_dir(&metadata.target_directory)?;

    let cov_dir = &metadata.target_directory.join("llvm-cov");
    fs::remove_dir_all(cov_dir)?;
    fs::create_dir(cov_dir)?;

    let target_dir = &metadata.target_directory.join("llvm-cov-target");

    if target_dir.exists() {
        for path in glob::glob(target_dir.join("*.profraw").as_str())?.filter_map(Result::ok) {
            fs::remove_file(path)?;
        }
    }
    fs::create_dir(target_dir)?;

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/source-based-code-coverage.html#including-doc-tests
    let doctests_dir = &target_dir.join("doctestbins");
    if args.doctests {
        fs::remove_dir_all(doctests_dir)?;
        fs::create_dir(doctests_dir)?;
    }

    let package_name = metadata.workspace_root.file_stem().unwrap();
    let profdata_file = &target_dir.join(format!("{}.profdata", package_name));
    fs::remove_file(profdata_file)?;
    let llvm_profile_file = target_dir.join(format!("{}-%m.profraw", package_name));

    let rustflags = &mut match env::var_os("RUSTFLAGS") {
        Some(rustflags) => rustflags,
        None => OsString::new(),
    };
    // --remap-path-prefix for Sometimes macros are displayed with abs path
    rustflags
        .push(format!(" -Zinstrument-coverage --remap-path-prefix {}/=", metadata.workspace_root));

    let rustdocflags = &mut env::var_os("RUSTDOCFLAGS");
    if args.doctests {
        let flags = rustdocflags.get_or_insert_with(OsString::new);
        flags.push(format!(
            " -Zinstrument-coverage -Zunstable-options --persist-doctests {}",
            doctests_dir
        ));
    }

    let cargo = cargo_binary();
    let mut cargo = ProcessBuilder::new(cargo);
    let version = String::from_utf8(cargo.arg("--version").run_with_output()?.stdout)?;
    if !version.contains("-nightly") && !version.contains("-dev") {
        cargo = ProcessBuilder::new("cargo");
        cargo.base_arg("+nightly");
    }
    cargo.dir(&metadata.workspace_root);

    cargo.env("RUSTFLAGS", rustflags);
    cargo.env("LLVM_PROFILE_FILE", &*llvm_profile_file);
    if let Some(rustdocflags) = rustdocflags {
        cargo.env("RUSTDOCFLAGS", rustdocflags);
    }

    cargo.args_replace(&["test", "--target-dir"]).arg(target_dir);
    append_args(&mut cargo, &args, &metadata);
    cargo.stdout_to_stderr = true;
    cargo.run()?;
    cargo.stdout_to_stderr = false;

    let output = cargo.arg("--no-run").arg("--message-format=json").run_with_output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let mut files = vec![];
    for (_, s) in stdout.lines().filter(|s| !s.is_empty()).enumerate() {
        let ar = serde_json::from_str::<Artifact>(s)?;
        if ar.profile.map_or(false, |p| p.test) {
            files.extend(ar.filenames.into_iter().filter(|s| !s.ends_with("dSYM")));
        }
    }
    if args.doctests {
        for f in glob::glob(doctests_dir.join("*/rust_out").as_str())?.filter_map(Result::ok) {
            if is_executable::is_executable(&f) {
                files.push(f.to_string_lossy().into_owned())
            }
        }
    }

    // Convert raw profile data.
    cargo
        .args_replace(&["profdata", "--", "merge", "-sparse"])
        .args(
            glob::glob(target_dir.join(format!("{}-*.profraw", package_name)).as_str())?
                .filter_map(Result::ok),
        )
        .arg("-o")
        .arg(profdata_file)
        .run()?;

    if args.json {
        cargo
            .args_replace(&[
                "cov",
                "--",
                "export",
                &format!("-instr-profile={}", profdata_file),
                "-format=text",
                "-summary-only",
                "-ignore-filename-regex",
                r".cargo/registry|.rustup/toolchains|test(s)?/",
                "-Xdemangler=rustfilt",
            ])
            .args(files.iter().flat_map(|f| vec!["-object", f]))
            .run()?;
    } else if args.text {
        cargo
            .args_replace(&[
                "cov",
                "--",
                "show",
                &format!("-instr-profile={}", profdata_file),
                "-show-line-counts-or-regions",
                "-show-instantiations",
                "-ignore-filename-regex",
                r".cargo/registry|.rustup/toolchains|test(s)?/",
                "-Xdemangler=rustfilt",
            ])
            .args(files.iter().flat_map(|f| vec!["-object", f]))
            .run()?;
    } else {
        cargo
            .args_replace(&[
                "cov",
                "--",
                "report",
                &format!("-instr-profile={}", profdata_file),
                "-ignore-filename-regex",
                r".cargo/registry|.rustup/toolchains|test(s)?/",
                "-Xdemangler=rustfilt",
            ])
            .args(files.iter().flat_map(|f| vec!["-object", f]))
            .run()?;
    }

    if args.html {
        cargo
            .args_replace(&[
                "cov",
                "--",
                "show",
                &format!("-instr-profile={}", profdata_file),
                "-format=html",
                &format!("-output-dir={}", cov_dir),
                "-show-expansions",
                "-show-instantiations",
                "-show-line-counts-or-regions",
                "-ignore-filename-regex",
                r".cargo/registry|.rustup/toolchains|test(s)?/",
                "-Xdemangler=rustfilt",
            ])
            .args(files.iter().flat_map(|f| vec!["-object", f]))
            .run()?;

        if args.open {
            open::that(cov_dir.join("index.html"))?;
        }
    }

    Ok(())
}

#[derive(Deserialize, Debug)]
struct Artifact {
    profile: Option<Profile>,
    #[serde(default)]
    filenames: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Profile {
    test: bool,
}

fn metadata(manifest_path: Option<&Path>) -> Result<cargo_metadata::Metadata> {
    let mut cmd = cargo_metadata::MetadataCommand::new();
    if let Some(path) = manifest_path {
        cmd.manifest_path(path);
    }
    Ok(cmd.exec()?)
}

fn append_args(cmd: &mut ProcessBuilder, args: &Args, metadata: &cargo_metadata::Metadata) {
    for exclude in &args.exclude {
        cmd.arg("--exclude");
        cmd.arg(exclude);
    }
    if args.workspace {
        cmd.arg("--workspace");
    }
    if args.release {
        cmd.arg("--release");
    }
    for features in &args.features {
        cmd.arg("--features");
        cmd.arg(features);
    }
    if args.all_features {
        cmd.arg("--all-features");
    }
    if args.no_default_features {
        cmd.arg("--no-default-features");
    }
    if let Some(target) = &args.target {
        cmd.arg("--target");
        cmd.arg(target);
    }
    if let Some(manifest_path) = &args.manifest_path {
        cmd.arg("--manifest-path");
        cmd.arg(manifest_path);
    }

    if !args.workspace && args.manifest_path.is_none() {
        if let Some(root) = &metadata.resolve.as_ref().unwrap().root {
            cmd.arg("--manifest-path");
            cmd.arg(&metadata[root].manifest_path);
        }
    }

    if !args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&args.args);
    }
}

fn cargo_binary() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

#![forbid(unsafe_code)]
#![warn(future_incompatible, rust_2018_idioms, single_use_lifetimes, unreachable_pub)]
#![warn(clippy::default_trait_access, clippy::wildcard_imports)]

// Refs:
// - https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html

mod fs;
mod process;

use std::{env, ffi::OsString};

use anyhow::Result;
use camino::Utf8Path;
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
    /// Export coverage data in "json" format (the report will be printed to stdout).
    ///
    /// This internally calls `llvm-cov export -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[structopt(long)]
    json: bool,
    /// Export coverage data in "lcov" format (the report will be printed to stdout).
    ///
    /// This internally calls `llvm-cov export -format=lcov`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.
    #[structopt(long, conflicts_with = "json")]
    lcov: bool,
    /// Export only summary information for each file in the coverage data.
    ///
    /// This flag can only be used together with either --json or --lcov.
    #[structopt(long)]
    summary_only: bool,

    /// Generate coverage reports in “text” format (the report will be printed to stdout).
    ///
    /// This internally calls `llvm-cov show -format=text`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[structopt(long, conflicts_with_all = &["json", "lcov"])]
    text: bool,
    /// Generate coverage reports in "html" format (the report will be generated in `target/llvm-cov` directory).
    ///
    /// This internally calls `llvm-cov show -format=html`.
    /// See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.
    #[structopt(long, conflicts_with_all = &["json", "lcov", "text"])]
    html: bool,
    /// Generate coverage reports in "html" format and open them in a browser after the operation.
    #[structopt(long, conflicts_with_all = &["json", "lcov", "text"])]
    open: bool,
    /// Specify a directory to write coverage reports into (default to `target/llvm-cov`).
    ///
    /// This flag can only be used together with --text, --html, or --open.
    #[structopt(long)]
    output_dir: Option<String>,

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
    /// Including doc tests (unstable)
    #[structopt(long)]
    doctests: bool,

    // ======================
    // Mapping cargo commands
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

impl Args {
    fn export(&self) -> bool {
        self.json || self.lcov
    }
    fn show(&self) -> bool {
        self.text || self.html
    }
}

fn main() -> Result<()> {
    let Opts::LlvmCov(mut args) = Opts::from_args();
    args.html |= args.open;
    if args.summary_only && !args.export() {
        eprintln!("--summary-only can only be used together with either --json or --lcov");
        std::process::exit(1);
    }
    if args.output_dir.is_some() && !args.show() {
        eprintln!("--output-dir can only be used together with --text, --html, or --open");
        std::process::exit(1);
    }

    let metadata = metadata(args.manifest_path.as_deref())?;
    fs::create_dir(&metadata.target_directory)?;

    let output_dir = match &args.output_dir {
        None if args.html => Some(metadata.target_directory.join("llvm-cov")),
        None => None,
        Some(output_dir) => Some(output_dir.into()),
    };
    if let Some(output_dir) = &output_dir {
        fs::remove_dir_all(output_dir)?;
        fs::create_dir_all(output_dir)?;
    }

    let target_dir = &metadata.target_directory.join("llvm-cov-target");

    if target_dir.exists() {
        for path in glob::glob(target_dir.join("*.profraw").as_str())?.filter_map(Result::ok) {
            fs::remove_file(path)?;
        }
    }
    fs::create_dir(target_dir)?;

    // https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html#including-doc-tests
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

    let format = Format::from_args(&args);
    format.run(&mut cargo, output_dir.as_deref(), args.summary_only, profdata_file, &files)?;

    if format == Format::Html {
        Format::None.run(
            &mut cargo,
            output_dir.as_deref(),
            args.summary_only,
            profdata_file,
            &files,
        )?;

        if args.open {
            open::that(output_dir.as_ref().unwrap().join("index.html"))?;
        }
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    /// `llvm-cov report`
    None,
    /// `llvm-cov export -format=text`
    Json,
    /// `llvm-cov export -format=lcov`
    LCov,
    /// `llvm-cov show -format=text`
    Text,
    /// `llvm-cov show -format=html`
    Html,
}

impl Format {
    fn from_args(args: &Args) -> Self {
        if args.json {
            Self::Json
        } else if args.lcov {
            Self::LCov
        } else if args.text {
            Self::Text
        } else if args.html {
            Self::Html
        } else {
            Self::None
        }
    }

    fn llvm_cov_args(&self) -> &'static [&'static str] {
        match self {
            Self::None => &["report"],
            Self::Json => &["export", "-format=text"],
            Self::LCov => &["export", "-format=lcov"],
            Self::Text => &["show", "-format=text"],
            Self::Html => &["show", "-format=html"],
        }
    }

    fn run(
        self,
        cargo: &mut ProcessBuilder,
        output_dir: Option<&Utf8Path>,
        summary_only: bool,
        profdata_file: &Utf8Path,
        files: &[String],
    ) -> Result<()> {
        cargo.args_replace(&["cov", "--"]).args(self.llvm_cov_args());

        match self {
            Self::Text | Self::Html => {
                cargo.args(&[
                    "-show-instantiations",
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                ]);
                if let Some(output_dir) = output_dir {
                    cargo.arg(&format!("-output-dir={}", output_dir));
                }
            }
            Self::Json | Self::LCov => {
                if summary_only {
                    cargo.arg("-summary-only");
                }
            }
            Self::None => {}
        }

        cargo
            .args(&[
                &format!("-instr-profile={}", profdata_file),
                "-ignore-filename-regex",
                r"rustc/|.cargo/registry|.rustup/toolchains|test(s)?/",
                "-Xdemangler=rustfilt",
            ])
            .args(files.iter().flat_map(|f| vec!["-object", f]))
            .run()
    }
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

fn metadata(manifest_path: Option<&str>) -> Result<cargo_metadata::Metadata> {
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

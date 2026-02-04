// SPDX-License-Identifier: Apache-2.0 OR MIT

// Refs:
// - https://llvm.org/docs/CommandGuide/llvm-profdata.html
// - https://llvm.org/docs/CommandGuide/llvm-cov.html

use std::{
    collections::{BTreeSet, HashMap},
    ffi::{OsStr, OsString},
    io::{self, BufRead as _, BufWriter, Read as _, Write as _},
    path::Path,
    time::SystemTime,
};

use anyhow::{Context as _, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_config2::Color;
use cargo_llvm_cov::json::{CodeCovJsonExport, CoverageKind, LlvmCovJsonExport};
use regex::Regex;
use serde_derive::Deserialize;
use tar::Archive;
use walkdir::WalkDir;

use crate::{
    cargo::Workspace,
    cli::ReportOptions,
    context::Context,
    demangler, env, fs,
    metadata::Metadata,
    os_str_to_str,
    regex_vec::{RegexVec, RegexVecBuilder},
    term,
};

pub(crate) fn generate(cx: &Context) -> Result<()> {
    if cx.args.report.no_report {
        return Ok(());
    }

    if let Some(output_dir) = &cx.args.report.output_dir {
        fs::create_dir_all(output_dir)?;
        if cx.args.report.html {
            fs::create_dir_all(output_dir.join("html"))?;
        }
        if cx.args.report.text {
            fs::create_dir_all(output_dir.join("text"))?;
        }
    }

    merge_profraw(cx).context("failed to merge profile data")?;

    let object_files = object_files(cx).context("failed to collect object files")?;
    let ignore_filename_regex = ignore_filename_regex(cx, &object_files)?;
    let format = ReportFormat::from_args(&cx.args.report);
    format
        .generate_report(cx, &object_files, ignore_filename_regex.as_deref())
        .context("failed to generate report")?;

    if cx.args.report.fail_under_functions.is_some()
        || cx.args.report.fail_under_lines.is_some()
        || cx.args.report.fail_under_regions.is_some()
        || cx.args.report.fail_uncovered_functions.is_some()
        || cx.args.report.fail_uncovered_lines.is_some()
        || cx.args.report.fail_uncovered_regions.is_some()
        || cx.args.report.show_missing_lines
    {
        let format = ReportFormat::Json;
        let json = format
            .get_json(cx, &object_files, ignore_filename_regex.as_ref())
            .context("failed to get json")?;

        if let Some(fail_under_functions) = cx.args.report.fail_under_functions {
            // Handle --fail-under-functions.
            let functions_percent = json
                .get_coverage_percent(CoverageKind::Functions)
                .context("failed to get function coverage")?;
            if functions_percent < fail_under_functions {
                term::error::set(true);
            }
        }

        if let Some(fail_under_lines) = cx.args.report.fail_under_lines {
            // Handle --fail-under-lines.
            let lines_percent = json
                .get_coverage_percent(CoverageKind::Lines)
                .context("failed to get line coverage")?;
            if lines_percent < fail_under_lines {
                term::error::set(true);
            }
        }

        if let Some(fail_under_regions) = cx.args.report.fail_under_regions {
            // Handle --fail-under-regions.
            let regions_percent = json
                .get_coverage_percent(CoverageKind::Regions)
                .context("failed to get region coverage")?;
            if regions_percent < fail_under_regions {
                term::error::set(true);
            }
        }

        if let Some(fail_uncovered_functions) = cx.args.report.fail_uncovered_functions {
            // Handle --fail-uncovered-functions.
            let uncovered =
                json.count_uncovered_functions().context("failed to count uncovered functions")?;
            if uncovered > fail_uncovered_functions {
                term::error::set(true);
            }
        }
        if let Some(fail_uncovered_lines) = cx.args.report.fail_uncovered_lines {
            // Handle --fail-uncovered-lines.
            let uncovered_files = json.get_uncovered_lines(ignore_filename_regex.as_deref());
            let uncovered = uncovered_files
                .iter()
                .fold(0_u64, |uncovered, (_, lines)| uncovered + lines.len() as u64);

            if uncovered > fail_uncovered_lines {
                term::error::set(true);
            }
        }
        if let Some(fail_uncovered_regions) = cx.args.report.fail_uncovered_regions {
            // Handle --fail-uncovered-regions.
            let uncovered =
                json.count_uncovered_regions().context("failed to count uncovered regions")?;
            if uncovered > fail_uncovered_regions {
                term::error::set(true);
            }
        }

        if cx.args.report.show_missing_lines {
            // Handle --show-missing-lines.
            let uncovered_files = json.get_uncovered_lines(ignore_filename_regex.as_deref());
            if !uncovered_files.is_empty() {
                let mut stdout = BufWriter::new(io::stdout().lock()); // Buffered because it is written with newline many times.
                writeln!(stdout, "Uncovered Lines:")?;
                for (file, lines) in &uncovered_files {
                    write!(stdout, "{file}: ")?;
                    let mut first = true;
                    for &l in lines {
                        if first {
                            first = false;
                        } else {
                            write!(stdout, ", ")?;
                        }
                        write!(stdout, "{l}")?;
                    }
                    writeln!(stdout)?;
                }
                stdout.flush()?;
            }
        }
    }

    if cx.args.report.open {
        let path = &cx.args.report.output_dir.as_ref().unwrap().join("html/index.html");
        status!("Opening", "{path}");
        open_report(cx, path)?;
    }
    Ok(())
}

fn open_report(cx: &Context, path: &Utf8Path) -> Result<()> {
    match &cx.ws.config.doc.browser {
        Some(browser) => {
            cmd!(&browser.path)
                .args(&browser.args)
                .arg(path)
                .run()
                .with_context(|| format!("couldn't open report with {}", browser.path.display()))?;
        }
        None => opener::open(path).context("couldn't open report")?,
    }
    Ok(())
}

fn merge_profraw(cx: &Context) -> Result<()> {
    // Convert raw profile data.
    let mut input_files = String::new();
    for path in glob::glob(
        Utf8Path::new(&glob::Pattern::escape(cx.ws.target_dir.as_str())).join("*.profraw").as_str(),
    )?
    .filter_map(Result::ok)
    {
        input_files.push_str(os_str_to_str(path.as_os_str())?);
        input_files.push('\n');
    }
    if input_files.is_empty() {
        if cx.ws.profdata_file.exists() {
            return Ok(());
        }
        bail!(
            "not found *.profraw files in {}; this may occur if target directory is accidentally \
             cleared, or running report subcommand without running any tests or binaries",
            cx.ws.target_dir
        );
    }
    let input_files_path = &cx.ws.target_dir.join(format!("{}-profraw-list", cx.ws.name));
    fs::write(input_files_path, input_files)?;
    let mut cmd = cx.process(&cx.llvm_profdata);
    cmd.args(["merge", "-sparse"])
        .arg("-f")
        .arg(input_files_path)
        .arg("-o")
        .arg(&cx.ws.profdata_file);
    if let Some(mode) = &cx.args.report.failure_mode {
        cmd.arg(format!("-failure-mode={mode}"));
    }
    if let Some(flags) = &cx.llvm_profdata_flags {
        cmd.args(flags.split(' ').filter(|s| !s.trim_start().is_empty()));
    }
    if term::verbose() {
        status!("Running", "{cmd}");
    }
    cmd.stdout_to_stderr().run()?;
    Ok(())
}

fn object_files(cx: &Context) -> Result<Vec<OsString>> {
    fn walk_target_dir<'a>(
        cx: &'a Context,
        build_script_re: &'a RegexVec,
        target_dir: &Utf8Path,
    ) -> impl Iterator<Item = walkdir::DirEntry> + 'a {
        WalkDir::new(target_dir)
            .into_iter()
            .filter_entry(move |e| {
                let p = e.path();
                // Refs: https://github.com/rust-lang/cargo/blob/0.85.0/src/cargo/core/compiler/layout.rs.
                if p.is_dir() {
                    if p.file_name().is_some_and(|f| {
                        f == "incremental"
                            || f == ".fingerprint"
                            || if cx.args.report.include_build_script {
                                f == "out"
                            } else {
                                f == "build"
                            }
                    }) {
                        // Ignore incremental compilation related files and output from build scripts.
                        return false;
                    }
                } else if cx.args.report.include_build_script {
                    if let (Some(stem), Some(p)) = (p.file_stem(), p.parent()) {
                        fn in_build_dir(p: &Path) -> bool {
                            let Some(p) = p.parent() else { return false };
                            let Some(f) = p.file_name() else { return false };
                            f == "build"
                        }
                        if in_build_dir(p) {
                            if stem == "build-script-build"
                                || stem
                                    .to_str()
                                    .unwrap_or_default()
                                    .starts_with("build_script_build-")
                            {
                                // TODO: use os_str_to_str?
                                let dir = p.file_name().unwrap().to_string_lossy();
                                if !build_script_re.is_match(&dir) {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .filter_map(Result::ok)
    }
    fn is_object(cx: &Context, f: &Path) -> bool {
        let ext = f.extension().unwrap_or_default();
        // We check extension instead of using is_executable crate because it always return true on WSL:
        // - https://github.com/taiki-e/cargo-llvm-cov/issues/316
        // - https://github.com/taiki-e/cargo-llvm-cov/issues/342
        if ext == "d" || ext == "rlib" || ext == "rmeta" || f.ends_with(".cargo-lock") {
            return false;
        }
        if cx.ws.target_is_windows
            && !(ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("dll"))
        {
            return false;
        }
        // Using std::fs instead of fs-err is okay here since we ignore error contents
        #[allow(clippy::disallowed_methods)]
        let Ok(metadata) = std::fs::metadata(f) else {
            return false;
        };
        if !metadata.is_file() {
            return false;
        }
        if cx.ws.target_is_windows {
            true
        } else {
            #[cfg(unix)]
            {
                // This is useless on WSL, but check for others just in case.
                use std::os::unix::fs::PermissionsExt as _;
                metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            true
        }
    }
    /// Make the path relative if it's a descendent of the current working dir, otherwise just return
    /// the original path
    fn make_relative<'a>(cx: &Context, p: &'a Path) -> &'a Path {
        p.strip_prefix(&cx.current_dir).unwrap_or(p)
    }

    let re = pkg_hash_re(cx)?;
    let build_script_re = build_script_hash_re(cx);
    let mut files = vec![];
    let mut searched_dir = String::new();
    // To support testing binary crate like tests that use the CARGO_BIN_EXE
    // environment variable, pass all compiled executables.
    // This is not the ideal way, but the way unstable book says it is cannot support them.
    // https://doc.rust-lang.org/nightly/rustc/instrument-coverage.html#tips-for-listing-the-binaries-automatically
    let mut target_dir = cx.ws.target_dir.clone();
    let build_dir = cx.ws.build_dir.clone();
    let mut auto_detect_profile = false;
    if cx.args.subcommand.read_nextest_archive() {
        // TODO: build-dir
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct BinariesMetadata {
            rust_build_meta: RustBuildMeta,
        }
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct RustBuildMeta {
            base_output_directories: Vec<String>,
        }
        target_dir.push("target");
        let archive_file = cx.args.nextest_archive_file.as_ref().unwrap();
        let file = fs::File::open(archive_file)?; // TODO: Buffering?
        let decoder = ruzstd::decoding::StreamingDecoder::new(file)?;
        let mut archive = Archive::new(decoder);
        let mut binaries_metadata = vec![];
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if path.ends_with("target/nextest/binaries-metadata.json") {
                entry.read_to_end(&mut binaries_metadata)?;
                break;
            }
        }
        if binaries_metadata.is_empty() {
            bail!("not found binaries-metadata.json in nextest archive {archive_file:?}");
        }
        match serde_json::from_slice::<BinariesMetadata>(&binaries_metadata) {
            // TODO: what multiple base_output_directories means?
            Ok(binaries_metadata)
                if binaries_metadata.rust_build_meta.base_output_directories.len() == 1 =>
            {
                target_dir.push(&binaries_metadata.rust_build_meta.base_output_directories[0]);
                auto_detect_profile = true;
            }
            res => {
                bail!(
                    "found binaries-metadata.json in nextest archive {archive_file:?}, but has unsupported or incompatible format: {res:?}"
                );
            }
        }
    }

    let mut collect_target_dir =
        |mut target_dir: Utf8PathBuf, mut build_dir: Option<Utf8PathBuf>| -> Result<()> {
            if !auto_detect_profile {
                // https://doc.rust-lang.org/nightly/cargo/reference/profiles.html#custom-profiles
                let profile = match cx.args.cargo_profile.as_deref() {
                    None if cx.args.release => "release",
                    Some("release" | "bench") => "release",
                    None | Some("dev" | "test") => "debug",
                    Some(p) => p,
                };
                target_dir.push(profile);
                if let Some(build_dir) = &mut build_dir {
                    build_dir.push(profile);
                }
            }
            for f in walk_target_dir(cx, &build_script_re, &target_dir) {
                let f = f.path();
                if is_object(cx, f) {
                    if let Some(file_stem) = fs::file_stem_recursive(f).unwrap().to_str() {
                        if re.is_match(file_stem) {
                            files.push(make_relative(cx, f).to_owned().into_os_string());
                        }
                    }
                }
            }
            searched_dir.push_str(target_dir.as_str());
            if let Some(build_dir) = &build_dir {
                if target_dir != *build_dir {
                    for f in walk_target_dir(cx, &build_script_re, build_dir) {
                        let f = f.path();
                        if is_object(cx, f) {
                            if let Some(file_stem) = fs::file_stem_recursive(f).unwrap().to_str() {
                                if re.is_match(file_stem) {
                                    files.push(make_relative(cx, f).to_owned().into_os_string());
                                }
                            }
                        }
                    }
                    searched_dir.push(',');
                    searched_dir.push_str(build_dir.as_str());
                }
            }
            Ok(())
        };
    // Check both host and target because proc-macro and build script are built for host.
    // https://doc.rust-lang.org/nightly/cargo/reference/build-cache.html
    if let Some(target) = &cx.args.target {
        let mut target_dir = target_dir.clone();
        let mut build_dir = build_dir.clone();
        target_dir.push(target);
        if let Some(build_dir) = &mut build_dir {
            build_dir.push(target);
        }
        collect_target_dir(target_dir, build_dir)?;
    }
    collect_target_dir(target_dir, build_dir)?;

    if cx.args.doctests {
        for f in glob::glob(
            Utf8Path::new(&glob::Pattern::escape(cx.ws.doctests_dir.as_str()))
                .join("*/rust_out")
                .as_str(),
        )?
        .filter_map(Result::ok)
        {
            if is_object(cx, &f) {
                files.push(make_relative(cx, &f).to_owned().into_os_string());
            }
        }
        searched_dir.push(',');
        searched_dir.push_str(cx.ws.doctests_dir.as_str());
    }

    // trybuild
    let trybuild_target_dir = cx.ws.trybuild_target_dir();
    let mut collect_trybuild_target_dir = |mut trybuild_target_dir: Utf8PathBuf| -> Result<()> {
        // Currently, trybuild always use debug build.
        trybuild_target_dir.push("debug");
        if trybuild_target_dir.is_dir() {
            let mut trybuild_targets = vec![];
            for metadata in trybuild_metadata(&cx.ws, &cx.ws.metadata.target_directory)? {
                for package in metadata.packages {
                    for target in package.targets {
                        trybuild_targets.push(target.name);
                    }
                }
            }
            if !trybuild_targets.is_empty() {
                let re = Regex::new(&format!("^({})(-[0-9a-f]+)?$", trybuild_targets.join("|")))
                    .unwrap();
                for entry in walk_target_dir(cx, &build_script_re, &trybuild_target_dir) {
                    let path = make_relative(cx, entry.path());
                    if let Some(file_stem) = fs::file_stem_recursive(path).unwrap().to_str() {
                        if re.is_match(file_stem) {
                            continue;
                        }
                    }
                    if is_object(cx, path) {
                        files.push(path.to_owned().into_os_string());
                    }
                }
                searched_dir.push(',');
                searched_dir.push_str(trybuild_target_dir.as_str());
            }
        }
        Ok(())
    };
    // Check both host and target because proc-macro and build script are built for host.
    if let Some(target) = &cx.args.target {
        let mut trybuild_target_dir = trybuild_target_dir.clone();
        trybuild_target_dir.push(target);
        collect_trybuild_target_dir(trybuild_target_dir)?;
    }
    collect_trybuild_target_dir(trybuild_target_dir)?;

    // ui_test
    let ui_test_target_dir = cx.ws.ui_test_target_dir();
    let mut collect_ui_test_target_dir = |ui_test_target_dir: Utf8PathBuf| -> Result<()> {
        if ui_test_target_dir.is_dir() {
            for entry in walk_target_dir(cx, &build_script_re, &ui_test_target_dir) {
                let path = make_relative(cx, entry.path());
                if is_object(cx, path) {
                    files.push(path.to_owned().into_os_string());
                }
            }
            searched_dir.push(',');
            searched_dir.push_str(ui_test_target_dir.as_str());
        }
        Ok(())
    };
    collect_ui_test_target_dir(ui_test_target_dir)?;

    // This sort is necessary to make the result of `llvm-cov show` match between macOS and Linux.
    files.sort_unstable();

    if files.is_empty() {
        bail!(
            "not found object files (searched directories: {searched_dir}); this may occur if \
             show-env subcommand is used incorrectly (see docs or other warnings), or unsupported \
             commands or configs are used",
        );
    }
    Ok(files)
}

fn pkg_hash_re(cx: &Context) -> Result<RegexVec> {
    let mut targets = BTreeSet::new();
    // Do not refer cx.workspace_members.include because it mixes --exclude and --exclude-from-report.
    for &id in &cx.ws.metadata.workspace_members {
        let pkg = &cx.ws.metadata[id];
        targets.insert(&pkg.name);
        for t in &pkg.targets {
            targets.insert(&t.name);
        }
    }
    let mut re = RegexVecBuilder::new("^(lib)?(", ")(-[0-9a-f]+)?$");
    for &t in &targets {
        re.or(&t.replace('-', "(-|_)"));
    }
    re.build()
}

fn build_script_hash_re(cx: &Context) -> RegexVec {
    let mut re = RegexVecBuilder::new("^(", ")-[0-9a-f]+$");
    for &id in &cx.workspace_members.included {
        re.or(&cx.ws.metadata[id].name);
    }
    re.build().unwrap()
}

/// Collects metadata for packages generated by trybuild. If the trybuild test
/// directory is not found, it returns an empty vector.
fn trybuild_metadata(ws: &Workspace, target_dir: &Utf8Path) -> Result<Vec<Metadata>> {
    // https://github.com/dtolnay/trybuild/pull/219
    let mut trybuild_dir = target_dir.join("tests").join("trybuild");
    if !trybuild_dir.is_dir() {
        trybuild_dir.pop();
        if !trybuild_dir.is_dir() {
            return Ok(vec![]);
        }
    }
    let mut metadata = vec![];
    for entry in fs::read_dir(trybuild_dir)?.filter_map(Result::ok) {
        let manifest_path = &entry.path().join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        metadata.push(Metadata::new(manifest_path, ws.config.cargo())?);
    }
    Ok(metadata)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ReportFormat {
    /// `llvm-cov report`
    None,
    /// `llvm-cov export -format=text`
    Json,
    /// `llvm-cov export -format=lcov`
    LCov,
    /// `llvm-cov export -format=lcov` later converted to XML
    Cobertura,
    /// `llvm-cov show -format=lcov` later converted to Codecov JSON
    Codecov,
    /// `llvm-cov show -format=text`
    Text,
    /// `llvm-cov show -format=html`
    Html,
}

impl ReportFormat {
    fn from_args(options: &ReportOptions) -> Self {
        if options.json {
            Self::Json
        } else if options.lcov {
            Self::LCov
        } else if options.cobertura {
            Self::Cobertura
        } else if options.codecov {
            Self::Codecov
        } else if options.text {
            Self::Text
        } else if options.html {
            Self::Html
        } else {
            Self::None
        }
    }

    const fn llvm_cov_args(self) -> &'static [&'static str] {
        match self {
            Self::None => &["report"],
            Self::Json | Self::Codecov => &["export", "-format=text"],
            Self::LCov | Self::Cobertura => &["export", "-format=lcov"],
            Self::Text => &["show", "-format=text"],
            Self::Html => &["show", "-format=html"],
        }
    }

    fn use_color(self, cx: &Context) -> Option<&'static str> {
        if matches!(self, Self::Json | Self::LCov | Self::Html) {
            // `llvm-cov export` doesn't have `-use-color` flag.
            // https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export
            // Color output cannot be disabled when generating html.
            return None;
        }
        if self == Self::Text && cx.args.report.output_dir.is_some() {
            return Some("-use-color=0");
        }
        match cx.ws.config.term.color {
            Some(Color::Auto) | None => None,
            Some(Color::Always) => Some("-use-color=1"),
            Some(Color::Never) => Some("-use-color=0"),
        }
    }

    fn generate_report(
        self,
        cx: &Context,
        object_files: &[OsString],
        ignore_filename_regex: Option<&str>,
    ) -> Result<()> {
        let mut cmd = cx.process(&cx.llvm_cov);

        cmd.args(self.llvm_cov_args());
        cmd.args(self.use_color(cx));
        cmd.arg(format!("-instr-profile={}", cx.ws.profdata_file));
        cmd.args(object_files.iter().flat_map(|f| [OsStr::new("-object"), f]));
        if let Some(ignore_filename_regex) = ignore_filename_regex {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename_regex);
        }

        match self {
            Self::Text | Self::Html => {
                cmd.args([
                    &format!("-show-instantiations={}", cx.args.report.show_instantiations),
                    "-show-line-counts-or-regions",
                    "-show-expansions",
                    "-show-branches=count",
                ]);
                if cmd!(&cx.llvm_cov, "show", "--help")
                    .read()
                    .unwrap_or_default()
                    .contains("-show-mcdc")
                {
                    // -show-mcdc requires LLVM 18+
                    cmd.arg("-show-mcdc");
                }
                let mut demangler = OsString::from("-Xdemangler=");
                demangler.push(&cx.current_exe);
                cmd.arg(demangler);
                demangler::set_env(&mut cmd);
                if let Some(output_dir) = &cx.args.report.output_dir {
                    if self == Self::Html {
                        cmd.arg(format!("-output-dir={}", output_dir.join("html")));
                    } else {
                        cmd.arg(format!("-output-dir={}", output_dir.join("text")));
                    }
                }
            }
            Self::Json | Self::LCov | Self::Cobertura | Self::Codecov => {
                if cx.args.report.summary_only {
                    cmd.arg("-summary-only");
                }
                if cx.args.report.skip_functions {
                    cmd.arg("-skip-functions");
                }
            }
            Self::None => {}
        }

        if let Some(flags) = &cx.llvm_cov_flags {
            cmd.args(flags.split(' ').filter(|s| !s.trim_start().is_empty()));
        }

        if cx.args.report.cobertura {
            if term::verbose() {
                status!("Running", "{cmd}");
            }
            let lcov = cmd.read()?;
            // Convert to XML
            let cdata = lcov2cobertura::parse_lines(
                lcov.as_bytes().lines(),
                &cx.ws.metadata.workspace_root,
                &[],
            )?;
            let demangler = lcov2cobertura::RustDemangler::new();
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .context("SystemTime before UNIX EPOCH!")?
                .as_secs();
            let out = lcov2cobertura::coverage_to_string(&cdata, now, demangler)?;

            if let Some(output_path) = &cx.args.report.output_path {
                fs::write(output_path, out)?;
                eprintln!();
                status!("Finished", "report saved to {output_path}");
            } else {
                // write XML to stdout
                println!("{out}");
            }
            return Ok(());
        }

        if cx.args.report.codecov {
            if term::verbose() {
                status!("Running", "{cmd}");
            }
            let cov = cmd.read()?;
            let cov: LlvmCovJsonExport = serde_json::from_str(&cov)?;
            let cov = CodeCovJsonExport::from_llvm_cov_json_export(cov, ignore_filename_regex);
            let out = serde_json::to_string(&cov)?;

            if let Some(output_path) = &cx.args.report.output_path {
                fs::write(output_path, out)?;
                eprintln!();
                status!("Finished", "report saved to {output_path}");
            } else {
                // write JSON to stdout
                println!("{out}");
            }
            return Ok(());
        }

        if let Some(output_path) = &cx.args.report.output_path {
            if term::verbose() {
                status!("Running", "{cmd}");
            }

            let out = cmd.read()?;
            if self == Self::Json {
                let mut cov = serde_json::from_str::<LlvmCovJsonExport>(&out)?;
                cov.inject(cx.ws.current_manifest.clone());
                fs::write(output_path, serde_json::to_string(&cov)?)?;
            } else {
                fs::write(output_path, out)?;
            }

            eprintln!();
            status!("Finished", "report saved to {output_path}");
            return Ok(());
        }

        if term::verbose() {
            status!("Running", "{cmd}");
        }

        if self == Self::Json {
            let out = cmd.read()?;
            let mut cov = serde_json::from_str::<LlvmCovJsonExport>(&out)?;
            cov.inject(cx.ws.current_manifest.clone());

            let mut stdout = BufWriter::new(io::stdout().lock()); // Buffered because it is written many times.
            serde_json::to_writer(&mut stdout, &cov)?;
            stdout.flush()?;
        } else {
            cmd.run()?;
        }

        if matches!(self, Self::Html | Self::Text) {
            if let Some(output_dir) = &cx.args.report.output_dir {
                eprintln!();
                if self == Self::Html {
                    status!("Finished", "report saved to {}", output_dir.join("html"));
                } else {
                    status!("Finished", "report saved to {}", output_dir.join("text"));
                }
            }
        }
        Ok(())
    }

    /// Generates JSON to perform further analysis on it.
    fn get_json(
        self,
        cx: &Context,
        object_files: &[OsString],
        ignore_filename_regex: Option<&String>,
    ) -> Result<LlvmCovJsonExport> {
        if let Self::Json = self {
        } else {
            bail!("requested JSON for non-JSON type");
        }

        let mut cmd = cx.process(&cx.llvm_cov);
        cmd.args(self.llvm_cov_args());
        cmd.arg(format!("-instr-profile={}", cx.ws.profdata_file));
        cmd.args(object_files.iter().flat_map(|f| [OsStr::new("-object"), f]));
        if let Some(ignore_filename_regex) = ignore_filename_regex {
            cmd.arg("-ignore-filename-regex");
            cmd.arg(ignore_filename_regex);
        }
        if term::verbose() {
            status!("Running", "{cmd}");
        }
        let cmd_out = cmd.read()?;
        let json = serde_json::from_str::<LlvmCovJsonExport>(&cmd_out)
            .context("failed to parse json from llvm-cov")?;
        Ok(json)
    }
}

fn ignore_filename_regex(cx: &Context, object_files: &[OsString]) -> Result<Option<String>> {
    // On Windows, we should escape the separator.
    const SEPARATOR: &str = if cfg!(windows) { "\\\\" } else { "/" };

    #[derive(Default)]
    struct Out(String);

    impl Out {
        fn push(&mut self, s: impl AsRef<str>) {
            if !self.0.is_empty() {
                self.0.push('|');
            }
            self.0.push_str(s.as_ref());
        }

        fn push_abs_path(&mut self, path: impl AsRef<Path>) {
            // TODO: use os_str_to_str?
            let path = regex::escape(&path.as_ref().to_string_lossy());
            let path = format!("^{path}($|{SEPARATOR})");
            self.push(path);
        }
    }

    let mut out = Out::default();

    if let Some(ignore_filename) = &cx.args.report.ignore_filename_regex {
        out.push(ignore_filename);
    }
    if !cx.args.report.no_default_ignore_filename_regex {
        let vendor_dirs =
            cx.ws.config.source.iter().filter_map(|(_, source)| source.directory.as_deref());

        // On Windows, file paths in cargo config.toml's can use `/` or `\` (when escaped as `\\`).
        // This value is going to be passed through into a regex, not through a path resolution step
        // that is agnostic to slash direction. llvm-cov uses paths with backslashes, which will
        // fail to match against a vendor directory like: `vendor/rust`. Both slash types are
        // reserved characters for file paths, meaning a naive string replacement can safely correct
        // the paths.
        // TODO: use os_str_to_str?
        #[cfg(windows)]
        let vendor_dirs = vendor_dirs
            .map(|dir| std::path::PathBuf::from(dir.to_string_lossy().replace("/", "\\")));

        vendor_dirs.for_each(|directory| out.push_abs_path(directory));

        if cx.args.dep_coverage.is_empty() {
            // TODO: Should we use the actual target path instead of using `tests|examples|benches`?
            //       We may have a directory like tests/support, so maybe we need both?
            if cx.args.remap_path_prefix {
                out.push(format!(
                    r"(^|{SEPARATOR})(rustc{SEPARATOR}([0-9a-f]+|[0-9]+\.[0-9]+\.[0-9]+)|tests|examples|benches){SEPARATOR}|{SEPARATOR}(tests\.rs|[0-9a-zA-Z_-]+[_-]tests\.rs)$"
                ));
            } else {
                out.push(format!(
                    r"{SEPARATOR}rustc{SEPARATOR}([0-9a-f]+|[0-9]+\.[0-9]+\.[0-9]+){SEPARATOR}|^{workspace_root}({SEPARATOR}.*)?{SEPARATOR}(tests|examples|benches){SEPARATOR}|^{workspace_root}({SEPARATOR}.*)?{SEPARATOR}(tests\.rs|[0-9a-zA-Z_-]+[_-]tests\.rs)$",
                    workspace_root = regex::escape(cx.ws.metadata.workspace_root.as_str())
                ));
            }
            out.push_abs_path(&cx.ws.target_dir);
            if let Some(build_dir) = &cx.ws.build_dir {
                if *build_dir != cx.ws.target_dir {
                    out.push_abs_path(build_dir);
                }
            }
            if cx.args.remap_path_prefix {
                if let Some(path) = env::home_dir() {
                    out.push_abs_path(path);
                }
            }
            if let Some(path) = env::cargo_home_with_cwd(&cx.current_dir) {
                // TODO: use os_str_to_str?
                let path = regex::escape(&path.as_os_str().to_string_lossy());
                let path = format!("^{path}{SEPARATOR}(registry|git){SEPARATOR}");
                out.push(path);
            }
            if let Some(path) = env::rustup_home_with_cwd(&cx.current_dir) {
                out.push_abs_path(path.join("toolchains"));
            }
            for path in resolve_excluded_paths(cx) {
                if cx.args.remap_path_prefix {
                    let path = path.strip_prefix(&cx.ws.metadata.workspace_root).unwrap_or(&path);
                    out.push_abs_path(path);
                } else {
                    out.push_abs_path(path);
                }
            }
        } else {
            let format = ReportFormat::Json;
            let json = format.get_json(cx, object_files, None).context("failed to get json")?;
            let crates_io_re = Regex::new(&format!(
                "{SEPARATOR}registry{SEPARATOR}src{SEPARATOR}index\\.crates\\.io-[0-9a-f]+{SEPARATOR}[0-9A-Za-z-_]+-[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z\\.-]+)?(\\+[0-9A-Za-z\\.-]+)?{SEPARATOR}"
            ))?;
            let dep_re = Regex::new(&format!(
                "{SEPARATOR}registry{SEPARATOR}src{SEPARATOR}index\\.crates\\.io-[0-9a-f]+{SEPARATOR}({})-[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z\\.-]+)?(\\+[0-9A-Za-z\\.-]+)?{SEPARATOR}",
                cx.args.dep_coverage.join("|")
            ))?;
            let mut set = BTreeSet::new();
            for data in &json.data {
                for file in &data.files {
                    // TODO: non-crates-io
                    if let Some(crates_io) = crates_io_re.find(&file.filename) {
                        if !dep_re.is_match(crates_io.as_str()) {
                            set.insert(regex::escape(crates_io.as_str()));
                        }
                    } else {
                        // TODO: dedup
                        set.insert(regex::escape(&file.filename));
                    }
                }
            }
            for f in set {
                out.push(f);
            }
        }
    }

    if out.0.is_empty() { Ok(None) } else { Ok(Some(out.0)) }
}

fn resolve_excluded_paths(cx: &Context) -> Vec<Utf8PathBuf> {
    let excluded: Vec<_> = cx
        .workspace_members
        .excluded
        .iter()
        .map(|&id| cx.ws.metadata[id].manifest_path.parent().unwrap())
        .collect();
    let included = cx
        .workspace_members
        .included
        .iter()
        .map(|&id| cx.ws.metadata[id].manifest_path.parent().unwrap());
    let mut excluded_path = vec![];
    let mut contains: HashMap<&Utf8Path, Vec<_>> = HashMap::default();
    for included in included {
        for &excluded in excluded.iter().filter(|e| included.starts_with(e)) {
            if let Some(v) = contains.get_mut(&excluded) {
                v.push(included);
            } else {
                contains.insert(excluded, vec![included]);
            }
        }
    }
    if contains.is_empty() {
        for &manifest_dir in &excluded {
            excluded_path.push(manifest_dir.to_owned());
        }
        return excluded_path;
    }

    for &excluded in &excluded {
        let Some(included) = contains.get(&excluded) else {
            excluded_path.push(excluded.to_owned());
            continue;
        };

        for _ in WalkDir::new(excluded).into_iter().filter_entry(|e| {
            let p = e.path();
            if !p.is_dir() {
                if p.extension().is_some_and(|e| e == "rs") {
                    excluded_path.push(p.to_owned().try_into().unwrap());
                }
                return false;
            }

            let mut contains = false;
            for included in included {
                if included.starts_with(p) {
                    if p.starts_with(included) {
                        return false;
                    }
                    contains = true;
                }
            }
            if contains {
                // continue to walk
                return true;
            }
            excluded_path.push(p.to_owned().try_into().unwrap());
            false
        }) {}
    }
    excluded_path
}

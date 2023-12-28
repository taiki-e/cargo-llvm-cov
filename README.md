# cargo-llvm-cov

[![crates.io](https://img.shields.io/crates/v/cargo-llvm-cov?style=flat-square&logo=rust)](https://crates.io/crates/cargo-llvm-cov)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![build status](https://img.shields.io/github/actions/workflow/status/taiki-e/cargo-llvm-cov/ci.yml?branch=main&style=flat-square&logo=github)](https://github.com/taiki-e/cargo-llvm-cov/actions)

Cargo subcommand to easily use LLVM source-based code coverage.

This is a wrapper around rustc [`-C instrument-coverage`][instrument-coverage] and provides:

- Generate very precise coverage data. (line coverage and region coverage)
- Support `cargo test`, `cargo run`, and [`cargo nextest`][nextest] with command-line interface compatible with cargo.
- Support for proc-macro, including coverage of UI tests.
- Support for doc tests. (this is currently optional and requires nightly, see [#2] for more)
- Fast because it does not introduce extra layers between rustc, cargo, and llvm-tools.

**Table of Contents:**

- [Usage](#usage)
  - [Basic usage](#basic-usage)
  - [Merge coverages generated under different test conditions](#merge-coverages-generated-under-different-test-conditions)
  - [Get coverage of C/C++ code linked to Rust library/binary](#get-coverage-of-cc-code-linked-to-rust-librarybinary)
  - [Get coverage of external tests](#get-coverage-of-external-tests)
  - [Exclude file from coverage](#exclude-file-from-coverage)
  - [Exclude function from coverage](#exclude-function-from-coverage)
  - [Continuous Integration](#continuous-integration)
  - [Display coverage in VS Code](#display-coverage-in-vs-code)
  - [Environment variables](#environment-variables)
  - [Additional JSON information](#additional-json-information)
- [Installation](#installation)
- [Known limitations](#known-limitations)
- [Related Projects](#related-projects)
- [License](#license)

## Usage

### Basic usage

<details>
<summary>Click to show a complete list of options</summary>

(See [docs](docs) directory for options of subcommands)

<!-- readme-long-help:start -->
```console
$ cargo llvm-cov --help
cargo-llvm-cov
Cargo subcommand to easily use LLVM source-based code coverage (-C instrument-coverage).

USAGE:
    cargo llvm-cov [SUBCOMMAND] [OPTIONS] [-- <args>...]

ARGS:
    <args>...
            Arguments for the test binary

OPTIONS:
        --json
            Export coverage data in "json" format

            If --output-path is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov export -format=text`. See
            <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.

        --lcov
            Export coverage data in "lcov" format

            If --output-path is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov export -format=lcov`. See
            <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.

        --cobertura
            Export coverage data in "cobertura" XML format

            If --output-path is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov export -format=lcov` and then converts to cobertura.xml.
            See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.

        --codecov
            Export coverage data in "Codecov Custom Coverage" format

            If --output-path is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov export -format=json` and then converts to codecov.json.
            See <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export> for more.

        --text
            Generate coverage report in “text” format

            If --output-path or --output-dir is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov show -format=text`. See
            <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.

        --html
            Generate coverage report in "html" format

            If --output-dir is not specified, the report will be generated in `target/llvm-cov/html`
            directory.

            This internally calls `llvm-cov show -format=html`. See
            <https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-show> for more.

        --open
            Generate coverage reports in "html" format and open them in a browser after the
            operation.

            See --html for more.

        --summary-only
            Export only summary information for each file in the coverage data

            This flag can only be used together with --json, --lcov, or --cobertura.

        --output-path <PATH>
            Specify a file to write coverage data into.

            This flag can only be used together with --json, --lcov, --cobertura, or --text.
            See --output-dir for --html and --open.

        --output-dir <DIRECTORY>
            Specify a directory to write coverage report into (default to `target/llvm-cov`).

            This flag can only be used together with --text, --html, or --open. See also
            --output-path.

        --failure-mode <any|all>
            Fail if `any` or `all` profiles cannot be merged (default to `any`)

        --ignore-filename-regex <PATTERN>
            Skip source code files with file paths that match the given regular expression

        --show-instantiations
            Show instantiations in report

        --no-cfg-coverage
            Unset cfg(coverage), which is enabled when code is built using cargo-llvm-cov

        --no-cfg-coverage-nightly
            Unset cfg(coverage_nightly), which is enabled when code is built using cargo-llvm-cov
            and nightly compiler

        --no-report
            Run tests, but don't generate coverage report

        --no-clean
            Build without cleaning any old build artifacts

        --fail-under-functions <MIN>
            Exit with a status of 1 if the total function coverage is less than MIN percent

        --fail-under-lines <MIN>
            Exit with a status of 1 if the total line coverage is less than MIN percent

        --fail-under-regions <MIN>
            Exit with a status of 1 if the total region coverage is less than MIN percent

        --fail-uncovered-lines <MAX>
            Exit with a status of 1 if the uncovered lines are greater than MAX

        --fail-uncovered-regions <MAX>
            Exit with a status of 1 if the uncovered regions are greater than MAX

        --fail-uncovered-functions <MAX>
            Exit with a status of 1 if the uncovered functions are greater than MAX

        --show-missing-lines
            Show lines with no coverage

        --include-build-script
            Include build script in coverage report

        --doctests
            Including doc tests (unstable)

            This flag is unstable. See <https://github.com/taiki-e/cargo-llvm-cov/issues/2> for
            more.

        --no-run
            Generate coverage report without running tests

        --no-fail-fast
            Run all tests regardless of failure

        --ignore-run-fail
            Run all tests regardless of failure and generate report

            If tests failed but report generation succeeded, exit with a status of 0.

    -q, --quiet
            Display one character per test instead of one line

        --lib
            Test only this package's library unit tests

        --bin <NAME>
            Test only the specified binary

        --bins
            Test all binaries

        --example <NAME>
            Test only the specified example

        --examples
            Test all examples

        --test <NAME>
            Test only the specified test target

        --tests
            Test all tests

        --bench <NAME>
            Test only the specified bench target

        --benches
            Test all benches

        --all-targets
            Test all targets

        --doc
            Test only this library's documentation (unstable)

            This flag is unstable because it automatically enables --doctests flag. See
            <https://github.com/taiki-e/cargo-llvm-cov/issues/2> for more.

    -p, --package <SPEC>
            Package to run tests for

        --workspace
            Test all packages in the workspace

        --all
            Alias for --workspace (deprecated)

        --exclude <SPEC>
            Exclude packages from both the test and report

        --exclude-from-test <SPEC>
            Exclude packages from the test (but not from the report)

        --exclude-from-report <SPEC>
            Exclude packages from the report (but not from the test)

    -j, --jobs <N>
            Number of parallel jobs, defaults to # of CPUs

    -r, --release
            Build artifacts in release mode, with optimizations

        --profile <PROFILE-NAME>
            Build artifacts with the specified profile

    -F, --features <FEATURES>
            Space or comma separated list of features to activate

        --all-features
            Activate all available features

        --no-default-features
            Do not activate the `default` feature

        --target <TRIPLE>
            Build for the target triple

            When this option is used, coverage for proc-macro and build script will not be displayed
            because cargo does not pass RUSTFLAGS to them.

        --coverage-target-only
            Activate coverage reporting only for the target triple

            Activate coverage reporting only for the target triple specified via `--target`. This is
            important, if the project uses multiple targets via the cargo bindeps feature, and not
            all targets can use `instrument-coverage`, e.g. a microkernel, or an embedded binary.

    -v, --verbose
            Use verbose output

            Use -vv (-vvv) to propagate verbosity to cargo.

        --color <WHEN>
            Coloring: auto, always, never

        --remap-path-prefix
            Use --remap-path-prefix for workspace root

            Note that this does not fully compatible with doctest.

        --include-ffi
            Include coverage of C/C++ code linked to Rust library/binary

            Note that `CC`/`CXX`/`LLVM_COV`/`LLVM_PROFDATA` environment variables must be set to
            Clang/LLVM compatible with the LLVM version used in rustc.

        --keep-going
            Do not abort the build as soon as there is an error (unstable)

        --ignore-rust-version
            Ignore `rust-version` specification in packages

        --manifest-path <PATH>
            Path to Cargo.toml

        --frozen
            Require Cargo.lock and cache are up to date

        --locked
            Require Cargo.lock is up to date

        --offline
            Run without accessing the network

    -Z <FLAG>
            Unstable (nightly-only) flags to Cargo, see 'cargo -Z help' for
            details

    -h, --help
            Print help information

    -V, --version
            Print version information

SUBCOMMANDS:
    test
            Run tests and generate coverage report
            This is equivalent to `cargo llvm-cov` without subcommand,
            except that test name filtering is supported.
    run
            Run a binary or example and generate coverage report
    report
            Generate coverage report
    show-env
            Output the environment set by cargo-llvm-cov to build Rust projects
    clean
            Remove artifacts that cargo-llvm-cov has generated in the past
    nextest
            Run tests with cargo nextest
            This internally calls `cargo nextest run`.
```
<!-- readme-long-help:end -->

</details>

By default, run tests (via `cargo test`), and print the coverage summary to stdout.

```sh
cargo llvm-cov
```

Currently, doc tests are disabled by default because nightly-only features are required to make coverage work for doc tests. see [#2] for more.

To run `cargo run` instead of `cargo test`, use `run` subcommand.

```sh
cargo llvm-cov run
```

With html report (the report will be generated to `target/llvm-cov/html` directory):

```sh
cargo llvm-cov --html
open target/llvm-cov/html/index.html
```

or

```sh
cargo llvm-cov --open
```

With plain text report (if `--output-path` is not specified, the report will be printed to stdout):

```sh
cargo llvm-cov --text | less -R
```

With json report (if `--output-path` is not specified, the report will be printed to stdout):

```sh
cargo llvm-cov --json --output-path cov.json
```

With lcov report (if `--output-path` is not specified, the report will be printed to stdout):

```sh
cargo llvm-cov --lcov --output-path lcov.info
```

You can get a coverage report in a different format based on the results of a previous run by using `cargo llvm-cov report`.

```sh
cargo llvm-cov --html          # run tests and generate html report
cargo llvm-cov report --lcov # generate lcov report
```

`cargo llvm-cov`/`cargo llvm-cov run`/`cargo llvm-cov nextest` cleans some build artifacts by default to avoid false positives/false negatives due to old build artifacts.
This behavior is disabled when `--no-clean`, `--no-report`, or `--no-run` is passed, and old build artifacts are retained.
When using these flags, it is recommended to first run `cargo llvm-cov clean --workspace` to remove artifacts that may affect the coverage results.

```sh
cargo llvm-cov clean --workspace # remove artifacts that may affect the coverage results
cargo llvm-cov --no-clean
```

### Merge coverages generated under different test conditions

You can merge the coverages generated under different test conditions by using `--no-report` and `cargo llvm-cov report`.

```sh
cargo llvm-cov clean --workspace # remove artifacts that may affect the coverage results
cargo llvm-cov --no-report --features a
cargo llvm-cov --no-report --features b
cargo llvm-cov report --lcov # generate report without tests
```

Note: To include coverage for doctests you also need to pass `--doctests` to `cargo llvm-cov report`.

### Get coverage of C/C++ code linked to Rust library/binary

Set `CC`, `CXX`, `LLVM_COV`, and `LLVM_PROFDATA` environment variables to Clang/LLVM compatible with the LLVM version used in rustc, and run cargo-llvm-cov with `--include-ffi` flag.

```sh
CC=<clang-path> \
CXX=<clang++-path> \
LLVM_COV=<llvm-cov-path> \
LLVM_PROFDATA=<llvm-profdata-path> \
  cargo llvm-cov --lcov --include-ffi
```

### Get coverage of external tests

`cargo test`, `cargo run`, and [`cargo nextest`][nextest] are available as builtin, but cargo-llvm-cov can also be used for arbitrary binaries built using cargo (including other cargo subcommands or external tests that use make, [xtask], etc.)

```sh
# Set the environment variables needed to get coverage.
source <(cargo llvm-cov show-env --export-prefix)
# Remove artifacts that may affect the coverage results.
# This command should be called after show-env.
cargo llvm-cov clean --workspace
# Above two commands should be called before build binaries.

cargo build # Build rust binaries.
# Commands using binaries in target/debug/*, including `cargo test` and other cargo subcommands.
# ...

cargo llvm-cov report --lcov # Generate report without tests.
```

Note: cargo-llvm-cov subcommands other than `report` and `clean` may not work correctly in the context where environment variables are set by `show-env`; consider using normal `cargo`/`cargo-nextest` commands.

Note: To include coverage for doctests you also need to pass `--doctests` to both `cargo llvm-cov show-env` and `cargo llvm-cov report`.

### Exclude file from coverage

To exclude specific file patterns from the report, use the `--ignore-filename-regex` option.

```sh
cargo llvm-cov --open --ignore-filename-regex build
```

### Exclude function from coverage

To exclude the specific function from coverage, use the [`#[coverage(off)]` attribute][rust-lang/rust#84605].

Since `#[coverage(off)]` is unstable, it is recommended to use it together with `cfg(coverage)` or `cfg(coverage_nightly)` set by cargo-llvm-cov.

```rust
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg_attr(coverage_nightly, coverage(off))]
fn exclude_from_coverage() {
    // ...
}
```

cfgs are set under the following conditions:

- `cfg(coverage)` is always set when using cargo-llvm-cov (unless `--no-cfg-coverage` flag passed)
- `cfg(coverage_nightly)` is set when using cargo-llvm-cov with nightly toolchain (unless `--no-cfg-coverage-nightly` flag passed)

If you want to ignore all `#[test]`-related code, consider using [coverage-helper] crate version 0.2+.

cargo-llvm-cov excludes code contained in the directory named `tests` from the report by default, so you can also use it instead of coverage-helper crate.

**Note:** `#[coverage(off)]` was previously named `#[no_coverage]`. When using `#[no_coverage]` in the old nightly, replace `feature(coverage_attribute)` with `feature(no_coverage)`, `coverage(off)` with `no_coverage`, and `coverage-helper` 0.2+ with `coverage-helper` 0.1.

### Continuous Integration

Here is an example of GitHub Actions workflow that uploads coverage to [Codecov].

```yaml
name: Coverage

on: [pull_request, push]

jobs:
  coverage:
    runs-on: ubuntu-latest
    env:
      CARGO_TERM_COLOR: always
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        run: rustup update stable
      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov
      - name: Generate code coverage
        run: cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v3
        with:
          token: ${{ secrets.CODECOV_TOKEN }} # not required for public repos
          files: lcov.info
          fail_ci_if_error: true
```

Currently, when using `--lcov` flag, [only line coverage is available on Codecov][#20].

By using `--codecov` flag instead of `--lcov` flag, you can use region coverage on Codecov:

```yaml
- name: Generate code coverage
  run: cargo llvm-cov --all-features --workspace --codecov --output-path codecov.json
- name: Upload coverage to Codecov
  uses: codecov/codecov-action@v3
  with:
    token: ${{ secrets.CODECOV_TOKEN }} # not required for public repos
    files: codecov.json
    fail_ci_if_error: true
```

Note that [the way Codecov shows region/branch coverage is not very good](https://github.com/taiki-e/cargo-llvm-cov/pull/255#issuecomment-1513318191).

### Display coverage in VS Code

You can display coverage in VS Code using [Coverage Gutters](https://marketplace.visualstudio.com/items?itemName=ryanluker.vscode-coverage-gutters).

Coverage Gutters supports lcov style coverage file and detects `lcov.info` files at the top level or in the `coverage` directory. Below is an example command to generate the coverage file.

```sh
cargo llvm-cov --lcov --output-path lcov.info
```

You may need to click the "Watch" label in the bottom bar of VS Code to display coverage.

### Environment variables

You can override these environment variables to change cargo-llvm-cov's behavior on your system:

- `CARGO_LLVM_COV_TARGET_DIR` -- Location of where to place all generated artifacts, relative to the current working directory. Default to `<cargo_target_dir>/llvm-cov-target`.
- `CARGO_LLVM_COV_SETUP` -- Control behavior if `llvm-tools-preview` component is not installed. See [#219] for more.
- `LLVM_COV` -- Override the path to `llvm-cov`. You may need to specify both this and `LLVM_PROFDATA` environment variables if you are using [`--include-ffi` flag](#get-coverage-of-cc-code-linked-to-rust-librarybinary) or if you are using a toolchain installed without via rustup.
- `LLVM_PROFDATA` -- Override the path to `llvm-profdata`. See `LLVM_COV` environment variable for more.
- `LLVM_COV_FLAGS` -- A space-separated list of additional flags to pass to all `llvm-cov` invocations that cargo-llvm-cov performs. See [LLVM documentation](https://llvm.org/docs/CommandGuide/llvm-cov.html) for available options.
- `LLVM_PROFDATA_FLAGS` -- A space-separated list of additional flags to pass to all `llvm-profdata` invocations that cargo-llvm-cov performs. See [LLVM documentation](https://llvm.org/docs/CommandGuide/llvm-profdata.html) for available options.

See also [environment variables that Cargo reads](https://doc.rust-lang.org/nightly/cargo/reference/environment-variables.html#environment-variables-cargo-reads). cargo-llvm-cov respects many of them.

### Additional JSON information

If **JSON** is selected as output format (with the `--json` flag), then cargo-llvm-cov will add additional contextual information at the root of the llvm-cov data. This can be helpful for programs that rely on the output of cargo-llvm-cov.

```json
{
  // Other regular llvm-cov fields ...
  "cargo_llvm_cov": {
    "version": "0.0.0",
    "manifest_path": "/path/to/your/project/Cargo.toml"
  }
}
```

- `version` specifies the version of cargo-llvm-cov that was used. This allows other programs to verify a certain version of it was used and make assertions of its behavior.
- `manifest_path` defines the absolute path to the Rust project's Cargo.toml that cargo-llvm-cov was executed on. It can help to avoid repeating the same option on both programs.

For example, when forwarding the JSON output directly to another program:

```sh
cargo-llvm-cov --json | some-program
```

## Installation

<!-- omit in toc -->
### Prerequisites

<!-- omit in toc -->
### From source

```sh
cargo +stable install cargo-llvm-cov --locked
```

Currently, installing cargo-llvm-cov requires rustc 1.70+.

cargo-llvm-cov is usually runnable with Cargo versions older than the Rust version
required for installation (e.g., `cargo +1.60 llvm-cov`). Currently, to run
cargo-llvm-cov requires Cargo 1.60+.

<!-- omit in toc -->
### From prebuilt binaries

You can download prebuilt binaries from the [Release page](https://github.com/taiki-e/cargo-llvm-cov/releases).
Prebuilt binaries are available for macOS, Linux (gnu and musl), and Windows (static executable).

<details>
<summary>Example of script to download cargo-llvm-cov</summary>

```sh
# Get host target
host=$(rustc -Vv | grep host | sed 's/host: //')
# Download binary and install to $HOME/.cargo/bin
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/taiki-e/cargo-llvm-cov/releases/latest/download/cargo-llvm-cov-$host.tar.gz | tar xzf - -C "$HOME/.cargo/bin"
```

</details>

<!-- omit in toc -->
### On GitHub Actions

You can use [taiki-e/install-action](https://github.com/taiki-e/install-action) to install prebuilt binaries on Linux, macOS, and Windows.
This makes the installation faster and may avoid the impact of [problems caused by upstream changes](https://github.com/tokio-rs/bytes/issues/506).

```yaml
- uses: taiki-e/install-action@cargo-llvm-cov
```

When used with [nextest]:

```yml
- uses: taiki-e/install-action@cargo-llvm-cov
- uses: taiki-e/install-action@nextest
```

<!-- omit in toc -->
### Via Homebrew

You can install [cargo-llvm-cov using Homebrew tap on macOS and Linux](https://github.com/taiki-e/homebrew-tap/blob/HEAD/Formula/cargo-llvm-cov.rb):

```sh
brew install taiki-e/tap/cargo-llvm-cov
```

<!-- omit in toc -->
### Via Scoop (Windows)

You can install [cargo-llvm-cov using Scoop](https://github.com/taiki-e/scoop-bucket/blob/HEAD/bucket/cargo-llvm-cov.json):

```sh
scoop bucket add taiki-e https://github.com/taiki-e/scoop-bucket
scoop install cargo-llvm-cov
```

<!-- omit in toc -->
### Via cargo-binstall

You can install cargo-llvm-cov using [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall cargo-llvm-cov
```

<!-- omit in toc -->
### Via pacman (Arch Linux)

You can install cargo-llvm-cov from the [extra repository](https://archlinux.org/packages/extra/x86_64/cargo-llvm-cov):

```sh
pacman -S cargo-llvm-cov
```

## Known limitations

- Branch coverage is not supported yet. See [#8] and [rust-lang/rust#79649] for more.
- Support for doc tests is unstable and has known issues. See [#2] and [rust-lang/rust#79417] for more.

See also [the code-coverage-related issues reported in rust-lang/rust](https://github.com/rust-lang/rust/labels/A-code-coverage).

## Related Projects

- [coverage-helper]: Helper for [#123].
- [cargo-config2]: Library to load and resolve Cargo configuration. cargo-llvm-cov uses this library.
- [cargo-hack]: Cargo subcommand to provide various options useful for testing and continuous integration.
- [cargo-minimal-versions]: Cargo subcommand for proper use of `-Z minimal-versions`.

[#2]: https://github.com/taiki-e/cargo-llvm-cov/issues/2
[#8]: https://github.com/taiki-e/cargo-llvm-cov/issues/8
[#20]: https://github.com/taiki-e/cargo-llvm-cov/issues/20
[#123]: https://github.com/taiki-e/cargo-llvm-cov/issues/123
[#219]: https://github.com/taiki-e/cargo-llvm-cov/issues/219
[cargo-config2]: https://github.com/taiki-e/cargo-config2
[cargo-hack]: https://github.com/taiki-e/cargo-hack
[cargo-minimal-versions]: https://github.com/taiki-e/cargo-minimal-versions
[codecov]: https://codecov.io
[coverage-helper]: https://github.com/taiki-e/coverage-helper
[instrument-coverage]: https://doc.rust-lang.org/rustc/instrument-coverage.html
[nextest]: https://nexte.st/book/test-coverage.html
[rust-lang/rust#79417]: https://github.com/rust-lang/rust/issues/79417
[rust-lang/rust#79649]: https://github.com/rust-lang/rust/issues/79649
[rust-lang/rust#84605]: https://github.com/rust-lang/rust/issues/84605
[xtask]: https://github.com/matklad/cargo-xtask

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

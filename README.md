# cargo-llvm-cov

[![crates.io](https://img.shields.io/crates/v/cargo-llvm-cov?style=flat-square&logo=rust)](https://crates.io/crates/cargo-llvm-cov)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![rustc](https://img.shields.io/badge/rustc-1.54+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![build status](https://img.shields.io/github/workflow/status/taiki-e/cargo-llvm-cov/CI/main?style=flat-square&logo=github)](https://github.com/taiki-e/cargo-llvm-cov/actions)

Cargo subcommand to easily use LLVM source-based code coverage.

This is a wrapper around rustc [`-Z instrument-coverage`][instrument-coverage] and provides:

- Generate very precise coverage data. (line coverage and region coverage)
- Support both `cargo test` and `cargo run`.
- Support for proc-macro, including coverage of UI tests.
- Support for doc tests. (this is currently optional, see [#2] for more)
- Command-line interface compatible with cargo.

**Table of Contents:**

- [Usage](#usage)
  - [Continuous Integration](#continuous-integration)
  - [Exclude function from coverage](#exclude-function-from-coverage)
- [Installation](#installation)
- [Known limitations](#known-limitations)
- [License](#license)

## Usage

<details>
<summary>Click to show a complete list of options</summary>

<!-- readme-long-help:start -->
```console
$ cargo llvm-cov --help
cargo-llvm-cov
Cargo subcommand to easily use LLVM source-based code coverage (-Z instrument-coverage).

Use -h for short descriptions and --help for more details.

USAGE:
    cargo llvm-cov [OPTIONS] [-- <ARGS>...] [SUBCOMMAND]

ARGS:
    <ARGS>...
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

            This flag can only be used together with either --json or --lcov.

        --output-path <PATH>
            Specify a file to write coverage data into.

            This flag can only be used together with --json, --lcov, or --text. See --output-dir for
            --html and --open.

        --output-dir <DIRECTORY>
            Specify a directory to write coverage report into (default to `target/llvm-cov`).

            This flag can only be used together with --text, --html, or --open. See also --output-
            path.

        --failure-mode <any|all>
            Fail if `any` or `all` profiles cannot be merged (default to `any`)

        --ignore-filename-regex <PATTERN>
            Skip source code files with file paths that match the given regular expression

        --no-report
            Run tests, but don't generate coverage report

        --doctests
            Including doc tests (unstable)

            This flag is unstable. See <https://github.com/taiki-e/cargo-llvm-cov/issues/2> for
            more.

        --no-run
            Generate coverage report without running tests

        --no-fail-fast
            Run all tests regardless of failure

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

            [aliases: all]

        --exclude <SPEC>
            Exclude packages from the test

    -j, --jobs <N>
            Number of parallel jobs, defaults to # of CPUs

        --release
            Build artifacts in release mode, with optimizations

        --profile <PROFILE-NAME>
            Build artifacts with the specified profile

        --features <FEATURES>
            Space or comma separated list of features to activate

        --all-features
            Activate all available features

        --no-default-features
            Do not activate the `default` feature

        --target <TRIPLE>
            Build for the target triple

            When this option is used, coverage for proc-macro and build script will not be displayed
            because cargo does not pass RUSTFLAGS to them.

    -v, --verbose
            Use verbose output

            Use -vv (-vvv) to propagate verbosity to cargo.

        --color <WHEN>
            Coloring

            [possible values: auto, always, never]

        --manifest-path <PATH>
            Path to Cargo.toml

        --frozen
            Require Cargo.lock and cache are up to date

        --locked
            Require Cargo.lock is up to date

        --offline
            Run without accessing the network

    -Z <FLAG>
            Unstable (nightly-only) flags to Cargo

    -h, --help
            Print help information

    -V, --version
            Print version information

SUBCOMMANDS:
    run
            Run a binary or example and generate coverage report
    clean
            Remove artifacts that cargo-llvm-cov has generated in the past
    help
            Print this message or the help of the given subcommand(s)
```
<!-- readme-long-help:end -->

</details>

By default, run tests (via `cargo test`), and print the coverage summary to stdout.

```sh
cargo llvm-cov
```

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

You can get a coverage report in a different format based on the results of a previous run by using `--no-run`.

```sh
cargo llvm-cov --html          # run tests and generate html report
cargo llvm-cov --no-run --lcov # generate lcov report
```

You can merge the coverages generated under different test conditions by using `--no-report` and `--no-run`.

```sh
cargo llvm-cov clean --workspace # remove artifacts that may affect the coverage results
cargo llvm-cov --no-report --features a
cargo llvm-cov --no-report --features b
cargo llvm-cov --no-run --lcov
```

In combination with the `show-env` subcommand, coverage can also be produced from arbitrary binaries:

```sh
cargo llvm-cov clean --workspace # remove artifacts that may affect the coverage results
source <(cargo llvm-cov show-env --export-prefix)
cargo build # build rust binaries
# commands using binaries in target/debug/*, including `cargo test`
# ...
cargo llvm-cov --no-run --lcov # generate report without tests
```

To exclude specific file patterns from the report, use the `--ignore-filename-regex` option.

```sh
cargo llvm-cov --open --ignore-filename-regex build
```

### Continuous Integration

Here is an example of GitHub Actions workflow that uploads coverage to [Codecov].

```yaml
name: Coverage

on: [pull_request, push]

jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install Rust
        run: rustup toolchain install nightly --component llvm-tools-preview
      - name: Install cargo-llvm-cov
        run: curl -LsSf https://github.com/taiki-e/cargo-llvm-cov/releases/latest/download/cargo-llvm-cov-x86_64-unknown-linux-gnu.tar.gz | tar xzf - -C ~/.cargo/bin
      - name: Generate code coverage
        run: cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v1
        with:
          token: ${{ secrets.CODECOV_TOKEN }} # not required for public repos
          files: lcov.info
          fail_ci_if_error: true
```

**NOTE:** Currently, only line coverage is available on Codecov. This is because `-Z instrument-coverage` does not support branch coverage and Codecov does not support region coverage. See also [#8], [#12], and [#20].

### Exclude function from coverage

To exclude the specific function from coverage, use the [`#[no_coverage]` attribute][rust-lang/rust#84605].

Since `#[no_coverage]` is unstable, it is recommended to use it together with `cfg(coverage)` set by cargo-llvm-cov.

```rust
#![cfg_attr(coverage, feature(no_coverage))]

#[cfg_attr(coverage, no_coverage)]
fn exclude_from_coverage() {
    // ...
}
```

## Installation

<!-- omit in toc -->
### Prerequisites

cargo-llvm-cov requires nightly
toolchain and llvm-tools-preview:

```sh
rustup component add llvm-tools-preview --toolchain nightly
```

<!-- omit in toc -->
### From source

```sh
cargo install cargo-llvm-cov
```

cargo-llvm-cov relies on unstable compiler flags so it requires a nightly
toolchain to be installed, though does not require nightly to be the default
toolchain or the one with which cargo-llvm-cov itself is executed. If the
default toolchain is one other than nightly, running `cargo llvm-cov` will find
and use nightly.

Currently, installing cargo-llvm-cov requires rustc 1.54+.

<!-- omit in toc -->
### From prebuilt binaries

You can download prebuilt binaries from the [Release page](https://github.com/taiki-e/cargo-llvm-cov/releases).
Prebuilt binaries are available for macOS, Linux (gnu and musl), and Windows (static executable).

<!-- omit in toc -->
### Via Homebrew

You can install cargo-llvm-cov using [Homebrew tap on macOS and Linux](https://github.com/taiki-e/homebrew-tap/blob/main/Formula/cargo-llvm-cov.rb):

```sh
brew install taiki-e/tap/cargo-llvm-cov
```

<!-- omit in toc -->
### Via AUR (ArchLinux)

You can install [cargo-llvm-cov from AUR](https://aur.archlinux.org/packages/cargo-llvm-cov):

```sh
paru -S cargo-llvm-cov
```

NOTE: AUR package is maintained by community, not maintainer of cargo-llvm-cov.

## Known limitations

- Due to a bug of `-Z instrument-coverage`, some files may be ignored. There is a known workaround for this issue, but note that the workaround is likely to cause another problem. See [rust-lang/rust#86177] and [#26] for more.
- Branch coverage is not supported yet. See [#8] and [rust-lang/rust#79649] for more.
- Support for doc tests is unstable and has known issues. See [#2] and [rust-lang/rust#79417] for more.

See also [the code-coverage-related issues reported in rust-lang/rust](https://github.com/rust-lang/rust/labels/A-code-coverage).

[#1]: https://github.com/taiki-e/cargo-llvm-cov/issues/1
[#2]: https://github.com/taiki-e/cargo-llvm-cov/issues/2
[#8]: https://github.com/taiki-e/cargo-llvm-cov/issues/8
[#12]: https://github.com/taiki-e/cargo-llvm-cov/issues/12
[#20]: https://github.com/taiki-e/cargo-llvm-cov/issues/20
[#26]: https://github.com/taiki-e/cargo-llvm-cov/issues/26
[codecov]: https://codecov.io
[instrument-coverage]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html
[rust-lang/rust#79417]: https://github.com/rust-lang/rust/issues/79417
[rust-lang/rust#79649]: https://github.com/rust-lang/rust/issues/79649
[rust-lang/rust#84605]: https://github.com/rust-lang/rust/issues/84605
[rust-lang/rust#86177]: https://github.com/rust-lang/rust/issues/86177

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

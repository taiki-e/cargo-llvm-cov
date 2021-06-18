# cargo-llvm-cov

[![crates.io](https://img.shields.io/crates/v/cargo-llvm-cov?style=flat-square&logo=rust)](https://crates.io/crates/cargo-llvm-cov)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![rustc](https://img.shields.io/badge/rustc-stable-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![build status](https://img.shields.io/github/workflow/status/taiki-e/cargo-llvm-cov/CI/main?style=flat-square&logo=github)](https://github.com/taiki-e/cargo-llvm-cov/actions)

**\[EXPERIMENTAL\]**
A wrapper for source based code coverage ([-Zinstrument-coverage][instrument-coverage], [rust-lang/rust#79121]).

## Installation

### Prerequisites

cargo-llvm-cov requires nightly
toolchain and llvm-tools-preview:

```sh
rustup component add llvm-tools-preview --toolchain nightly
```

### From source

```sh
cargo install cargo-llvm-cov --version 0.1.0-alpha.4
```

cargo-llvm-cov relies on unstable compiler flags so it requires a nightly
toolchain to be installed, though does not require nightly to be the default
toolchain or the one with which cargo-llvm-cov itself is executed. If the default
toolchain is one other than nightly, running `cargo llvm-cov` will find and use
nightly anyway.

### From prebuilt binaries

You can download prebuilt binaries from the [Release page](https://github.com/taiki-e/cargo-llvm-cov/releases).

## Usage

<details>
<summary>Click to show a complete list of options</summary>

```console
$ cargo llvm-cov --help
cargo-llvm-cov
A wrapper for source based code coverage (-Zinstrument-coverage).

Use -h for short descriptions and --help for more details.

USAGE:
    cargo llvm-cov [OPTIONS] [-- <args>...]

OPTIONS:
        --json
            Export coverage data in "json" format

            If --output-path is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov export -format=text`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-export> for more.
        --lcov
            Export coverage data in "lcov" format.

            If --output-path is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov export -format=lcov`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-export> for more.
        --text
            Generate coverage reports in “text” format.

            If --output-path or --output-dir is not specified, the report will be printed to stdout.

            This internally calls `llvm-cov show -format=text`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-show> for more.
        --html
            Generate coverage reports in "html" format. If --output-dir is not specified, the report will be generated
            in `target/llvm-cov` directory.

            This internally calls `llvm-cov show -format=html`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-show> for more.
        --open
            Generate coverage reports in "html" format and open them in a browser after the operation.

            See --html for more.
        --summary-only
            Export only summary information for each file in the coverage data.

            This flag can only be used together with either --json or --lcov.
        --output-path <PATH>
            Specify a file to write coverage data into.

            This flag can only be used together with --json, --lcov, or --text. See --output-dir for --html and --open.
        --output-dir <DIRECTORY>
            Specify a directory to write coverage reports into (default to `target/llvm-cov`).

            This flag can only be used together with --text, --html, or --open. See also --output-path.
        --ignore-filename-regex <PATTERN>
            Skip source code files with file paths that match the given regular expression

        --doctests
            Including doc tests (unstable)

        --no-fail-fast
            Run all tests regardless of failure

        --workspace
            Test all packages in the workspace [aliases: all]

        --exclude <SPEC>...
            Exclude packages from the test

        --release
            Build artifacts in release mode, with optimizations

        --features <FEATURES>...
            Space or comma separated list of features to activate

        --all-features
            Activate all available features

        --no-default-features
            Do not activate the `default` feature

        --target <TRIPLE>
            Build for the target triple

        --manifest-path <PATH>
            Path to Cargo.toml

    -v, --verbose
            Use verbose output (-vv very verbose/build.rs output)

        --color <WHEN>
            Coloring: auto, always, never

        --frozen
            Require Cargo.lock and cache are up to date

        --locked
            Require Cargo.lock is up to date

    -Z <FLAG>...
            Unstable (nightly-only) flags to Cargo

    -h, --help
            Prints help information

    -V, --version
            Prints version information


ARGS:
    <args>...
            Arguments for the test binary
```

</details>

By default, only the summary is printed to stdout.

```sh
cargo llvm-cov
```

With html report (the report will be generated to `target/llvm-cov` directory):

```sh
cargo llvm-cov --html
open target/llvm-cov/index.html
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
        run: curl -LsSf https://github.com/taiki-e/cargo-llvm-cov/releases/download/v0.1.0-alpha.4/cargo-llvm-cov-x86_64-unknown-linux-gnu.tar.gz | tar xzf - -C ~/.cargo/bin
      - name: Generate code coverage
        run: cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v1
        with:
          token: ${{ secrets.CODECOV_TOKEN }} # not required for public repos
          files: lcov.info
          fail_ci_if_error: true
```

Note: Currently, only line coverage is available on Codecov. This is because `-Zinstrument-coverage` does not support branch coverage and Codecov does not support region coverage. See also [#8], [#12], and [#20].

## Known limitations

- Due to a bug of `-Zinstrument-coverage`, some files may be ignored. There is a known workaround for this issue, but note that the workaround is likely to cause another problem. See [rust-lang/rust#86177] and [#26] for more.
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
[rust-lang/rust#79121]: https://github.com/rust-lang/rust/issues/79121
[rust-lang/rust#79417]: https://github.com/rust-lang/rust/issues/79417
[rust-lang/rust#79649]: https://github.com/rust-lang/rust/issues/79649
[rust-lang/rust#86177]: https://github.com/rust-lang/rust/issues/86177

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

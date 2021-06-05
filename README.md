# cargo-llvm-cov

[![crates.io](https://img.shields.io/crates/v/cargo-llvm-cov?style=flat-square&logo=rust)](https://crates.io/crates/cargo-llvm-cov)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![rustc](https://img.shields.io/badge/rustc-stable-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![build status](https://img.shields.io/github/workflow/status/taiki-e/cargo-llvm-cov/CI/main?style=flat-square&logo=github)](https://github.com/taiki-e/cargo-llvm-cov/actions)

**\[EXPERIMENTAL\]**
A wrapper for source based code coverage ([-Zinstrument-coverage][instrument-coverage], [rust-lang/rust#79121]).

## Installation

cargo-llvm-cov currently requires llvm-tools-preview and [rustfilt](https://github.com/luser/rustfilt).

```sh
cargo install cargo-llvm-cov --version 0.1.0-alpha.3

cargo install rustfilt

rustup component add llvm-tools-preview
```

Alternatively, download compiled binaries from [GitHub Releases](https://github.com/taiki-e/cargo-llvm-cov/releases).

cargo-llvm-cov relies on unstable compiler flags so it requires a nightly
toolchain to be installed, though does not require nightly to be the default
toolchain or the one with which cargo-llvm-cov itself is executed. If the default
toolchain is one other than nightly, running `cargo llvm-cov` will find and use
nightly anyway.

## Usage

<details>
<summary>A complete list of options</summary>

```console
$ cargo llvm-cov --help
cargo-llvm-cov
A wrapper for source based code coverage (-Zinstrument-coverage)

USAGE:
    cargo llvm-cov [OPTIONS] [-- <args>...]

OPTIONS:
        --json
            Export coverage data in "json" format (the report will be printed to stdout).

            This internally calls `llvm-cov export -format=text`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-export> for more.
        --lcov
            Export coverage data in "lcov" format (the report will be printed to stdout).

            This internally calls `llvm-cov export -format=lcov`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-export> for more.
        --summary-only
            Export only summary information for each file in the coverage data.

            This flag can only be used together with either --json or --lcov.
        --text
            Generate coverage reports in “text” format (the report will be printed to stdout).

            This internally calls `llvm-cov show -format=text`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-show> for more.
        --html
            Generate coverage reports in "html" format (the report will be generated in `target/llvm-cov` directory).

            This internally calls `llvm-cov show -format=html`. See <https://llvm.org/docs/CommandGuide/llvm-
            cov.html#llvm-cov-show> for more.
        --open
            Generate coverage reports in "html" format and open them in a browser after the operation

        --output-dir <output-dir>
            Specify a directory to write coverage reports into (default to `target/llvm-cov`).

            This flag can only be used together with --text, --html, or --open.
        --doctests
            Including doc tests (unstable)

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

        --color <WHEN>
            Coloring: auto, always, never

    -h, --help
            Prints help information

    -V, --version
            Prints version information


ARGS:
    <args>...
            Arguments for the test binary
```

</details>

By default, only the summary is displayed in the terminal.

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

With plain text report (the report will be printed to stdout):

```sh
cargo llvm-cov --text | less -R
```

With json report (the report will be printed to stdout):

```sh
cargo llvm-cov --json
```

With lcov report (the report will be printed to stdout):

```sh
cargo llvm-cov --lcov
```

## Known limitations

- Branch coverage is not supported yet. See [#8] and [rust-lang/rust#79649] for more.
- Support for doc tests is unstable and has known issues. See [#2] and [rust-lang/rust#79417] for more.
- Binary crates (`cargo run`) are not supported yet. See [#1] for more.
- Procedural macros are not supported yet.

See also [the coverage-related issues reported in rust-lang/rust](https://github.com/rust-lang/rust/labels/A-code-coverage).

[#1]: https://github.com/taiki-e/cargo-llvm-cov/issues/1
[#2]: https://github.com/taiki-e/cargo-llvm-cov/issues/2
[#8]: https://github.com/taiki-e/cargo-llvm-cov/issues/8
[instrument-coverage]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/instrument-coverage.html
[rust-lang/rust#79121]: https://github.com/rust-lang/rust/issues/79121
[rust-lang/rust#79417]: https://github.com/rust-lang/rust/issues/79417
[rust-lang/rust#79649]: https://github.com/rust-lang/rust/issues/79649

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

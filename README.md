# cargo-llvm-cov

[![crates.io](https://img.shields.io/crates/v/cargo-llvm-cov?style=flat-square&logo=rust)](https://crates.io/crates/cargo-llvm-cov)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![rustc](https://img.shields.io/badge/rustc-stable-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![build status](https://img.shields.io/github/workflow/status/taiki-e/cargo-llvm-cov/CI/main?style=flat-square&logo=github)](https://github.com/taiki-e/cargo-llvm-cov/actions)
![maintenance-status](https://img.shields.io/badge/maintenance-experimental-blue?style=flat-square)

**\[EXPERIMENTAL\]**
A wrapper for [source based code coverage (-Zinstrument-coverage)][source-based-code-coverage].

## Installation

```sh
cargo install cargo-llvm-cov --version 0.1.0-alpha.1

cargo install cargo-binutils

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

*See `cargo llvm-cov --help` for a complete list of options*

By default, only the summary is displayed in the terminal.

```sh
cargo llvm-cov
```

With html report (report will be generated to `target/llvm-cov`):

```sh
cargo llvm-cov --html
open target/llvm-cov/index.html
```

or

```sh
cargo llvm-cov --html --open
```

With json report:

```sh
cargo llvm-cov --json
```

[source-based-code-coverage]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/source-based-code-coverage.html

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

<!--
Note: In this file, do not use the hard wrap in the middle of a sentence for compatibility with GitHub comment style markdown rendering.
-->

## [Unreleased]

## [0.1.4] - 2021-08-29

- [Improve heuristics around artifact cleanup.](https://github.com/taiki-e/cargo-llvm-cov/pull/79)
  This removes the need to recompile dependencies in most cases.

- [Fix an issue where `--package` option could not handle package specifications containing the version such as `futures:0.3.16`.](https://github.com/taiki-e/cargo-llvm-cov/pull/80)

## [0.1.3] - 2021-08-26

- [Add `--verbose` option to `cargo llvm-cov clean` subcommand.](https://github.com/taiki-e/cargo-llvm-cov/pull/75)

- Fix regressions introduced in 0.1.2. ([#74](https://github.com/taiki-e/cargo-llvm-cov/pull/74), [#76](https://github.com/taiki-e/cargo-llvm-cov/pull/76))

## [0.1.2] - 2021-08-26

**Note: This release has been yanked due to regressions fixed in 0.1.3.**

- [Set `cfg(coverage)` to easily use `#[no_coverage]`.](https://github.com/taiki-e/cargo-llvm-cov/pull/72)

- [Add `--quiet`, `--doc`, and `--jobs` options.](https://github.com/taiki-e/cargo-llvm-cov/pull/70)

- [Add `cargo llvm-cov clean` subcommand.](https://github.com/taiki-e/cargo-llvm-cov/pull/73)

## [0.1.1] - 2021-08-25

- [Add `--lib`, `--bin`, `--bins`, `--example`, `--examples`, `--test`, `--tests`, `--bench`, `--benches`, `--all-targets`, `--profile`, and `--offline` options.](https://github.com/taiki-e/cargo-llvm-cov/pull/67)

- [Respect `BROWSER` environment variable and `doc.browser` cargo config.](https://github.com/taiki-e/cargo-llvm-cov/pull/66)

## [0.1.0] - 2021-08-15

- [Update clap to fix build error.](https://github.com/taiki-e/cargo-llvm-cov/pull/59)

- [Support latest version of trybuild.](https://github.com/taiki-e/cargo-llvm-cov/pull/54)

- [Change output directory of `--html` and `--open` options from `target/llvm-cov` to `target/llvm-cov/html`.](https://github.com/taiki-e/cargo-llvm-cov/pull/62)

- [You can now merge the coverages generated under different test conditions by using `--no-report` and `--no-run`.](https://github.com/taiki-e/cargo-llvm-cov/pull/55)

  ```sh
  cargo clean
  cargo llvm-cov --no-report --features a
  cargo llvm-cov --no-report --features b
  cargo llvm-cov --no-run --lcov
  ```

- [Add environment variables to pass additional flags to llvm-cov/llvm-profdata.](https://github.com/taiki-e/cargo-llvm-cov/pull/58)

  - `CARGO_LLVM_COV_FLAGS` to pass additional flags to llvm-cov. (value: space-separated list)
  - `CARGO_LLVM_PROFDATA_FLAGS` to pass additional flags to llvm-profdata. (value: space-separated list)

- [Fix "Failed to load coverage" error when together used with trybuild.](https://github.com/taiki-e/cargo-llvm-cov/pull/49)

- [Fix bug in `--exclude` and `--package` options](https://github.com/taiki-e/cargo-llvm-cov/pull/56)

- [Fix bug in color-detection when both `--text` and `--output-dir` used.](https://github.com/taiki-e/cargo-llvm-cov/pull/62)

- [`--html` and `--open` options no longer outputs a summary at the same time.](https://github.com/taiki-e/cargo-llvm-cov/pull/61)

- [Respect rustflags and rustdocflags set by cargo config file.](https://github.com/taiki-e/cargo-llvm-cov/pull/52)

- Diagnostic improvements.

## [0.1.0-alpha.5] - 2021-08-07

- [Support Windows.](https://github.com/taiki-e/cargo-llvm-cov/pull/41)

- [Support trybuild.](https://github.com/taiki-e/cargo-llvm-cov/pull/44)

- [Fix mapping error in `--doctests` option.](https://github.com/taiki-e/cargo-llvm-cov/pull/40)

- [Fix bug in `--target` option.](https://github.com/taiki-e/cargo-llvm-cov/pull/46)

- [Add `--package` option.](https://github.com/taiki-e/cargo-llvm-cov/pull/42)

## [0.1.0-alpha.4] - 2021-06-13

- [cargo-llvm-cov no longer requires rustfilt.](https://github.com/taiki-e/cargo-llvm-cov/pull/29)

- [Acknowledge that procedural macros are supported.](https://github.com/taiki-e/cargo-llvm-cov/pull/27)

- [Fix support of testing binary crate](https://github.com/taiki-e/cargo-llvm-cov/pull/23)

- [Fix an issue where git dependencies were displayed in the coverage report.](https://github.com/taiki-e/cargo-llvm-cov/pull/19)

- [Fix an issue where path dependencies that not included in the workspace were displayed in coverage report.](https://github.com/taiki-e/cargo-llvm-cov/pull/25)

- [Fix bug in `--exclude` option.](https://github.com/taiki-e/cargo-llvm-cov/pull/30)

- Fix several bugs.

- [Add `--output-path` option to specify a file to write coverage data into.](https://github.com/taiki-e/cargo-llvm-cov/pull/18)

- [Add `--ignore-filename-regex` option to skip specified source code files from coverage report.](https://github.com/taiki-e/cargo-llvm-cov/pull/19)

- [Add `--color` option.](https://github.com/taiki-e/cargo-llvm-cov/pull/15)

- [Add `--no-fail-fast`, `--frozen`, and `--locked` option.](https://github.com/taiki-e/cargo-llvm-cov/pull/16)

- [Add `--verbose` flag.](https://github.com/taiki-e/cargo-llvm-cov/pull/19)

- [Improve diagnostics when the required tools are not installed.](https://github.com/taiki-e/cargo-llvm-cov/pull/17)

## [0.1.0-alpha.3] - 2021-06-04

- [cargo-llvm-cov no longer requires cargo-binutils.](https://github.com/taiki-e/cargo-llvm-cov/pull/11)

- [`--json` flag now exports all coverage data by default.](https://github.com/taiki-e/cargo-llvm-cov/pull/9) If you want to get only summary information, use `--summary-only` flag together.

- [Enable `--html` flag automatically when `--open` flag is passed.](https://github.com/taiki-e/cargo-llvm-cov/pull/5)

- [Add `--lcov` flag for exporting coverage data in "lcov" format.](https://github.com/taiki-e/cargo-llvm-cov/pull/9)

- [Add `--output-dir` flag for specifying a directory to write coverage reports generated by `--html` or `--text` flag.](https://github.com/taiki-e/cargo-llvm-cov/pull/9)

- [Fix a bug in cargo version detection.](https://github.com/taiki-e/cargo-llvm-cov/pull/7)

- [Fix an issue where llvm-cov's auto-detection of color output doesn't work.](https://github.com/taiki-e/cargo-llvm-cov/pull/11)

- Fix several bugs.

- Documentation improvements.

## [0.1.0-alpha.2] - 2021-02-12

- [Add `--text` option to output full report in plain text.](https://github.com/taiki-e/cargo-llvm-cov/pull/3)

## [0.1.0-alpha.1] - 2021-01-23

Initial release

[Unreleased]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.0-alpha.5...v0.1.0
[0.1.0-alpha.5]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.0-alpha.4...v0.1.0-alpha.5
[0.1.0-alpha.4]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.0-alpha.3...v0.1.0-alpha.4
[0.1.0-alpha.3]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.0-alpha.2...v0.1.0-alpha.3
[0.1.0-alpha.2]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[0.1.0-alpha.1]: https://github.com/taiki-e/cargo-llvm-cov/releases/tag/v0.1.0-alpha.1

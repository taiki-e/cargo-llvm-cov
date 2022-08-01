# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

<!--
Note: In this file, do not use the hard wrap in the middle of a sentence for compatibility with GitHub comment style markdown rendering.
-->

## [Unreleased]

## [0.4.13] - 2022-08-01

- Fix an issue where merging of multiple `cargo llvm-cov run` coverage did not work.

## [0.4.12] - 2022-07-30

- Support `target.<cfg>.rustflags`. ([#200](https://github.com/taiki-e/cargo-llvm-cov/pull/200))

- Remove workaround for an old rustc bug on Windows if unnecessary. ([#199](https://github.com/taiki-e/cargo-llvm-cov/pull/199), thanks @ldm0)

## [0.4.11] - 2022-07-20

- Fix handling of existing CFLAGS/CXXFLAGS when `--include-ffi` flag is passed. ([#196](https://github.com/taiki-e/cargo-llvm-cov/pull/196))

## [0.4.10] - 2022-07-18

- Support coverage of C/C++ code linked to Rust library/binary. ([#194](https://github.com/taiki-e/cargo-llvm-cov/pull/194))

## [0.4.9] - 2022-07-07

- Fix an issue where some files were incorrectly ignored in reports. ([#191](https://github.com/taiki-e/cargo-llvm-cov/pull/191))

## [0.4.8] - 2022-06-16

- Correctly escape regular expressions passed to `-ignore-filename-regex`. ([#188](https://github.com/taiki-e/cargo-llvm-cov/pull/188), thanks @rhysd)

## [0.4.7] - 2022-06-13

- Pin clap to 3.1. ([#185](https://github.com/taiki-e/cargo-llvm-cov/pull/185))

## [0.4.6] - 2022-06-13

- Improve `--show-missing-lines` for multiple functions in a single line. ([#183](https://github.com/taiki-e/cargo-llvm-cov/pull/183), thanks @vmiklos)

- Limit the number of test threads to work around [rust-lang/rust#91092](https://github.com/rust-lang/rust/issues/91092). ([#184](https://github.com/taiki-e/cargo-llvm-cov/pull/184))

## [0.4.5] - 2022-06-02

- Fix handling of `RUSTC_WRAPPER`, `RUSTC`, and similar environment variables and configs. ([#180](https://github.com/taiki-e/cargo-llvm-cov/pull/180))

- Distribute prebuilt binaries for aarch64 macOS. ([#179](https://github.com/taiki-e/cargo-llvm-cov/pull/179))

## [0.4.4] - 2022-05-30

- Add `--fail-uncovered-{lines,regions,functions}` options to set the exit code based on uncovered {lines,regions,functions}. ([#173](https://github.com/taiki-e/cargo-llvm-cov/pull/173))

- Add `--ignore-run-fail` option to generate coverage even if tests fail. ([#174](https://github.com/taiki-e/cargo-llvm-cov/pull/174))

## [0.4.3] - 2022-05-29

- Fix metadata for cargo binstall. ([#176](https://github.com/taiki-e/cargo-llvm-cov/pull/176))

## [0.4.2] - 2022-05-29

- Add metadata for cargo binstall. ([#175](https://github.com/taiki-e/cargo-llvm-cov/pull/175), thanks @vmiklos)

## [0.4.1] - 2022-05-24

- Add `--coverage-target-only` flag to use rustflags only for target. ([#167](https://github.com/taiki-e/cargo-llvm-cov/pull/167), thanks @haraldh)

## [0.4.0] - 2022-05-12

- cargo-llvm-cov no longer changes the current directory when running cargo. ([#161](https://github.com/taiki-e/cargo-llvm-cov/pull/161))

- Exclude build script from report by default. ([#163](https://github.com/taiki-e/cargo-llvm-cov/pull/163))
  You can use `--include-build-script` flag to include build script in report.

- Set `cfg(coverage_nightly)` when nightly compiler is used. ([#164](https://github.com/taiki-e/cargo-llvm-cov/pull/164))

- Support short flags of `--release` (`-r`) and `--features` (`-F`). ([#165](https://github.com/taiki-e/cargo-llvm-cov/pull/165))

- Support [custom profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#custom-profiles). ([#166](https://github.com/taiki-e/cargo-llvm-cov/pull/166))

## [0.3.3] - 2022-05-06

- Fix an issue where codes in the target directory are not being properly excluded from reports when using `show-env` subcommand. ([#156](https://github.com/taiki-e/cargo-llvm-cov/pull/156))

## [0.3.2] - 2022-05-05

- Alleviate an issue where "File name or extension is too long" error occurs in Windows. ([#155](https://github.com/taiki-e/cargo-llvm-cov/pull/155))

## [0.3.1] - 2022-05-01

- Calculate `--show-missing-lines` based on function regions. ([#150](https://github.com/taiki-e/cargo-llvm-cov/pull/150), thanks @vmiklos)

## [0.3.0] - 2022-04-08

- cargo-llvm-cov now always select the current toolchain. ([#148](https://github.com/taiki-e/cargo-llvm-cov/pull/148))

  Previously, if `-C instrument-coverage` is not available in the current toolchain, the nightly toolchain was used. (See [release note of 0.2.0](https://github.com/taiki-e/cargo-llvm-cov/releases/tag/v0.2.0) for more information on the previous behavior.)

- Make `--remap-path-prefix` optional. ([#141](https://github.com/taiki-e/cargo-llvm-cov/pull/141))

  Previously this flag was always used, but due to some bugs discovered we decided to disable it by default. If you were dependent on the behavior provided by this flag, you can use the same behavior by passing the `--remap-path-prefix` flag to cargo-llvm-cov.

- Stabilize a few unstable options.

## [0.2.4] - 2022-03-18

- Add support for `nextest`. ([#144](https://github.com/taiki-e/cargo-llvm-cov/pull/144), thanks @skyzh)

## [0.2.3] - 2022-03-05

- Add `--show-missing-lines` option to show uncovered lines in the command-line output. ([#143](https://github.com/taiki-e/cargo-llvm-cov/pull/143), thanks @vmiklos)

## [0.2.2] - 2022-03-01

- Add `--fail-under-lines` option to set the exit code based on coverage percentage. ([#139](https://github.com/taiki-e/cargo-llvm-cov/pull/139), thanks @vmiklos)

## [0.2.1] - 2022-02-18

- Update clap to 3.1. ([#136](https://github.com/taiki-e/cargo-llvm-cov/pull/136))

## [0.2.0] - 2022-02-06

- Update to stabilized `-C instrument-coverage`. ([#130](https://github.com/taiki-e/cargo-llvm-cov/pull/130))

  Support for `-Z instrument-coverage` in the old nightly will also be kept for compatibility.

  **Compatibility Note:** In 0.2, if `-C instrument-coverage` or `-Z instrument-coverage` is not available in the default toolchain, running `cargo llvm-cov` will find and use nightly (this is almost the same behavior as 0.1). This behavior is necessary because only the recent nightly currently supports `-C instrument-coverage` (and also for compatibility with 0.1). This behavior will be changed in 0.3 to always select the default toolchain. If you are likely to be affected by the change in 0.3, cargo-llvm-cov will emit a warning. 0.3 is planned to be released after `-C instrument-coverage` is available in the stable toolchain.

- Remove support of multiple values in `--package` and `--exclude`. ([#133](https://github.com/taiki-e/cargo-llvm-cov/pull/133))

  [This behavior was unintentionally enabled in the older version of 0.1 and was deprecated in the recent version of 0.1.](https://github.com/taiki-e/cargo-llvm-cov/pull/127#issuecomment-1018204521)

- Add `--exclude-from-test` option to exclude specific packages from the test but not from the report. ([#131](https://github.com/taiki-e/cargo-llvm-cov/pull/131))

- Add `--exclude-from-report` option to exclude specific packages from the report but not from the test. ([#131](https://github.com/taiki-e/cargo-llvm-cov/pull/131))

- Workspace members are now always included in the report unless specified by `--exclude` or `--exclude-from-report`. ([#131](https://github.com/taiki-e/cargo-llvm-cov/pull/131))

## [0.1.16] - 2022-01-21

- Alleviate an issue where "File name or extension is too long" error occurs in Windows. ([#126](https://github.com/taiki-e/cargo-llvm-cov/pull/126), thanks @aganders3)

- Re-enable multiple values for `--package` and `--exclude`. ([#127](https://github.com/taiki-e/cargo-llvm-cov/pull/127), thanks @aganders3)

  This behavior was unintentionally enabled in older versions and disabled in recent versions.

  We will support this again in 0.1.x for compatibility, but will remove it in 0.2.x.

- Distribute prebuilt binaries for aarch64 Linux (gnu and musl).

## [0.1.15] - 2022-01-06

- Fix bug in `show-env` subcommand. ([#121](https://github.com/taiki-e/cargo-llvm-cov/pull/121))

## [0.1.14] - 2022-01-06

- Add `show-env` subcommand. ([#115](https://github.com/taiki-e/cargo-llvm-cov/pull/115), thanks @davidhewitt)

- cargo-llvm-cov no longer sets `CARGO_TARGET_DIR`. ([#112](https://github.com/taiki-e/cargo-llvm-cov/pull/112), thanks @smoelius)

- cargo-llvm-cov can now properly exclude arbitrary `CARGO_HOME` and `RUSTUP_HOME` from reports.

## [0.1.13] - 2021-12-14

- Support custom-built rust toolchain. ([#111](https://github.com/taiki-e/cargo-llvm-cov/pull/111), thanks @tofay)

## [0.1.12] - 2021-11-15

- Exclude `CARGO_HOME` and `RUSTUP_HOME` used in the official rust docker image from reports. ([#105](https://github.com/taiki-e/cargo-llvm-cov/pull/105))

## [0.1.11] - 2021-11-13

- Fix ["conflicting weak extern definition" error](https://github.com/rust-lang/rust/issues/85461) on windows. ([#101](https://github.com/taiki-e/cargo-llvm-cov/pull/101))

## [0.1.10] - 2021-10-24

- Fix a compatibility issue with `cc`. ([#98](https://github.com/taiki-e/cargo-llvm-cov/pull/98))

## [0.1.9] - 2021-10-13

- Distribute statically linked binary on Windows MSVC. ([#95](https://github.com/taiki-e/cargo-llvm-cov/pull/95))

## [0.1.8] - 2021-10-04

- Fix an issue where some files were incorrectly ignored in reports. ([#94](https://github.com/taiki-e/cargo-llvm-cov/pull/94), thanks @larsluthman)

## [0.1.7] - 2021-09-19

- Add `--failure-mode` option. ([#91](https://github.com/taiki-e/cargo-llvm-cov/pull/91), thanks @smoelius)

## [0.1.6] - 2021-09-03

- Add `cargo llvm-cov run` subcommand to get coverage of `cargo run`. ([#89](https://github.com/taiki-e/cargo-llvm-cov/pull/89))

## [0.1.5] - 2021-09-01

- Add `--workspace` flag to `cargo llvm-cov clean` subcommand. ([#85](https://github.com/taiki-e/cargo-llvm-cov/pull/85))

- Fix bug around artifact cleanup. ([#85](https://github.com/taiki-e/cargo-llvm-cov/pull/85))

## [0.1.4] - 2021-08-29

- Improve heuristics around artifact cleanup. ([#79](https://github.com/taiki-e/cargo-llvm-cov/pull/79))
  This removes the need to recompile dependencies in most cases.

- Fix an issue where `--package` option could not handle package specifications containing the version such as `futures:0.3.16`. ([#80](https://github.com/taiki-e/cargo-llvm-cov/pull/80))

## [0.1.3] - 2021-08-26

- Add `--verbose` option to `cargo llvm-cov clean` subcommand. ([#75](https://github.com/taiki-e/cargo-llvm-cov/pull/75))

- Fix regressions introduced in 0.1.2. ([#74](https://github.com/taiki-e/cargo-llvm-cov/pull/74), [#76](https://github.com/taiki-e/cargo-llvm-cov/pull/76))

## [0.1.2] - 2021-08-26

**Note:** This release has been yanked due to regressions fixed in 0.1.3.

- Set `cfg(coverage)` to easily use `#[no_coverage]`. ([#72](https://github.com/taiki-e/cargo-llvm-cov/pull/72))

- Add `--quiet`, `--doc`, and `--jobs` options. ([#70](https://github.com/taiki-e/cargo-llvm-cov/pull/70))

- Add `cargo llvm-cov clean` subcommand. ([#73](https://github.com/taiki-e/cargo-llvm-cov/pull/73))

## [0.1.1] - 2021-08-25

- Add `--lib`, `--bin`, `--bins`, `--example`, `--examples`, `--test`, `--tests`, `--bench`, `--benches`, `--all-targets`, `--profile`, and `--offline` options. ([#67](https://github.com/taiki-e/cargo-llvm-cov/pull/67))

- Respect `BROWSER` environment variable and `doc.browser` cargo config. ([#66](https://github.com/taiki-e/cargo-llvm-cov/pull/66))

## [0.1.0] - 2021-08-15

- Update clap to fix build error. ([#59](https://github.com/taiki-e/cargo-llvm-cov/pull/59))

- Support latest version of trybuild. ([#54](https://github.com/taiki-e/cargo-llvm-cov/pull/54))

- Change output directory of `--html` and `--open` options from `target/llvm-cov` to `target/llvm-cov/html`. ([#62](https://github.com/taiki-e/cargo-llvm-cov/pull/62))

- You can now merge the coverages generated under different test conditions by using `--no-report` and `--no-run`. ([#55](https://github.com/taiki-e/cargo-llvm-cov/pull/55))

  ```sh
  cargo clean
  cargo llvm-cov --no-report --features a
  cargo llvm-cov --no-report --features b
  cargo llvm-cov --no-run --lcov
  ```

- Add environment variables to pass additional flags to llvm-cov/llvm-profdata. ([#58](https://github.com/taiki-e/cargo-llvm-cov/pull/58))

  - `CARGO_LLVM_COV_FLAGS` to pass additional flags to llvm-cov. (value: space-separated list)
  - `CARGO_LLVM_PROFDATA_FLAGS` to pass additional flags to llvm-profdata. (value: space-separated list)

- Fix "Failed to load coverage" error when together used with trybuild. ([#49](https://github.com/taiki-e/cargo-llvm-cov/pull/49))

- Fix bug in `--exclude` and `--package` options. ([#56](https://github.com/taiki-e/cargo-llvm-cov/pull/56))

- Fix bug in color-detection when both `--text` and `--output-dir` used. ([#62](https://github.com/taiki-e/cargo-llvm-cov/pull/62))

- `--html` and `--open` options no longer outputs a summary at the same time. ([#61](https://github.com/taiki-e/cargo-llvm-cov/pull/61))

- Respect rustflags and rustdocflags set by cargo config file. ([#52](https://github.com/taiki-e/cargo-llvm-cov/pull/52))

- Diagnostic improvements.

## [0.1.0-alpha.5] - 2021-08-07

- Support Windows. ([#41](https://github.com/taiki-e/cargo-llvm-cov/pull/41))

- Support trybuild. ([#44](https://github.com/taiki-e/cargo-llvm-cov/pull/44))

- Fix mapping error in `--doctests` option. ([#40](https://github.com/taiki-e/cargo-llvm-cov/pull/40))

- Fix bug in `--target` option. ([#46](https://github.com/taiki-e/cargo-llvm-cov/pull/46))

- Add `--package` option. ([#42](https://github.com/taiki-e/cargo-llvm-cov/pull/42))

## [0.1.0-alpha.4] - 2021-06-13

- cargo-llvm-cov no longer requires rustfilt. ([#29](https://github.com/taiki-e/cargo-llvm-cov/pull/29))

- Acknowledge that procedural macros are supported. ([#27](https://github.com/taiki-e/cargo-llvm-cov/pull/27))

- Fix support of testing binary crate. ([#23](https://github.com/taiki-e/cargo-llvm-cov/pull/23))

- Fix an issue where git dependencies were displayed in the coverage report. ([#19](https://github.com/taiki-e/cargo-llvm-cov/pull/19))

- Fix an issue where path dependencies that not included in the workspace were displayed in coverage report. ([#25](https://github.com/taiki-e/cargo-llvm-cov/pull/25))

- Fix bug in `--exclude` option. ([#30](https://github.com/taiki-e/cargo-llvm-cov/pull/30))

- Fix several bugs.

- Add `--output-path` option to specify a file to write coverage data into. ([#18](https://github.com/taiki-e/cargo-llvm-cov/pull/18))

- Add `--ignore-filename-regex` option to skip specified source code files from coverage report. ([#19](https://github.com/taiki-e/cargo-llvm-cov/pull/19))

- Add `--color` option. ([#15](https://github.com/taiki-e/cargo-llvm-cov/pull/15))

- Add `--no-fail-fast`, `--frozen`, and `--locked` option. ([#16](https://github.com/taiki-e/cargo-llvm-cov/pull/16))

- Add `--verbose` flag. ([#19](https://github.com/taiki-e/cargo-llvm-cov/pull/19))

- Improve diagnostics when the required tools are not installed. ([#17](https://github.com/taiki-e/cargo-llvm-cov/pull/17))

## [0.1.0-alpha.3] - 2021-06-04

- cargo-llvm-cov no longer requires cargo-binutils. ([#11](https://github.com/taiki-e/cargo-llvm-cov/pull/11))

- `--json` flag now exports all coverage data by default. ([#9](https://github.com/taiki-e/cargo-llvm-cov/pull/9))
  If you want to get only summary information, use `--summary-only` flag together.

- Enable `--html` flag automatically when `--open` flag is passed. ([#5](https://github.com/taiki-e/cargo-llvm-cov/pull/5))

- Add `--lcov` flag for exporting coverage data in "lcov" format. ([#9](https://github.com/taiki-e/cargo-llvm-cov/pull/9))

- Add `--output-dir` flag for specifying a directory to write coverage reports generated by `--html` or `--text` flag. ([#9](https://github.com/taiki-e/cargo-llvm-cov/pull/9))

- Fix a bug in cargo version detection. ([#7](https://github.com/taiki-e/cargo-llvm-cov/pull/7))

- Fix an issue where llvm-cov's auto-detection of color output doesn't work. ([#11](https://github.com/taiki-e/cargo-llvm-cov/pull/11))

- Fix several bugs.

- Documentation improvements.

## [0.1.0-alpha.2] - 2021-02-12

- Add `--text` option to output full report in plain text. ([#3](https://github.com/taiki-e/cargo-llvm-cov/pull/3), thanks @romac)

## [0.1.0-alpha.1] - 2021-01-23

Initial release

[Unreleased]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.13...HEAD
[0.4.13]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.12...v0.4.13
[0.4.12]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.11...v0.4.12
[0.4.11]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.10...v0.4.11
[0.4.10]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.9...v0.4.10
[0.4.9]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.8...v0.4.9
[0.4.8]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.7...v0.4.8
[0.4.7]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.6...v0.4.7
[0.4.6]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.5...v0.4.6
[0.4.5]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.4...v0.4.5
[0.4.4]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.2.4...v0.3.0
[0.2.4]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.16...v0.2.0
[0.1.16]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.15...v0.1.16
[0.1.15]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.14...v0.1.15
[0.1.14]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.13...v0.1.14
[0.1.13]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.12...v0.1.13
[0.1.12]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.11...v0.1.12
[0.1.11]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/taiki-e/cargo-llvm-cov/compare/v0.1.4...v0.1.5
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

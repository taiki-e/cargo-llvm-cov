// SPDX-License-Identifier: Apache-2.0 OR MIT

// Refs: https://github.com/llvm/llvm-project/blob/llvmorg-18.1.2/llvm/tools/llvm-cov/CoverageExporterJson.cpp
// TODO: reflect https://github.com/llvm/llvm-project/commit/8ecbb0404d740d1ab173554e47cef39cd5e3ef8c#diff-e5de2b538138d03e13b43901f61adc61992516c742991ebaf1a13f2f8623910a?

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
};

use anyhow::{Context as _, Result};
use camino::Utf8PathBuf;
use regex::Regex;
use serde::ser::{Serialize, SerializeMap as _, Serializer};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct LlvmCovJsonExport {
    /// List of one or more export objects
    pub data: Vec<Export>,
    // llvm.coverage.json.export
    #[serde(rename = "type")]
    type_: String,
    version: String,
    /// Additional information injected into the export data.
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    cargo_llvm_cov: Option<CargoLlvmCov>,
}

/// <https://docs.codecov.com/docs/codecov-custom-coverage-format>
///
/// This represents the fraction: `{covered}/{count}`.
#[derive(Debug, Default)]
struct CodeCovCoverage {
    count: u64,
    covered: u64,
}

impl Serialize for CodeCovCoverage {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}/{}", self.covered, self.count))
    }
}

/// line -> coverage in fraction
#[derive(Default)]
struct CodeCovExport(BTreeMap<u64, CodeCovCoverage>);

/// Custom serialize [`CodeCovExport`] as "string" -> JSON (as function)
/// Serialize as "string" -> JSON
impl Serialize for CodeCovExport {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(&key.to_string(), value)?;
        }
        map.end()
    }
}

#[derive(Default, Serialize)]
pub struct CodeCovJsonExport {
    /// filename -> list of uncovered lines.
    coverage: BTreeMap<String, CodeCovExport>,
}

impl CodeCovJsonExport {
    fn from_export(value: Export, ignore_filename_regex: Option<&Regex>) -> Self {
        let functions = value.functions.unwrap_or_default();

        let mut regions = HashMap::new();

        for func in functions {
            for filename in func.filenames {
                if let Some(re) = ignore_filename_regex {
                    if re.is_match(&filename) {
                        continue;
                    }
                }
                // region location to covered
                let coverage: &mut HashMap<RegionLocation, bool> =
                    regions.entry(filename).or_default();
                for region in &func.regions {
                    let loc = RegionLocation::from(region);

                    let covered = coverage.entry(loc).or_default();

                    *covered = *covered || region.execution_count() > 0;
                }
            }
        }

        let mut coverage = BTreeMap::new();

        for (filename, regions) in regions {
            let coverage: &mut CodeCovExport = coverage.entry(filename).or_default();

            for (loc, covered) in regions {
                for line in loc.lines() {
                    let coverage = coverage.0.entry(line).or_default();
                    coverage.count += 1;
                    coverage.covered += covered as u64;
                }
            }
        }

        Self { coverage }
    }

    #[must_use]
    pub fn from_llvm_cov_json_export(
        value: LlvmCovJsonExport,
        ignore_filename_regex: Option<&str>,
    ) -> Self {
        let re = ignore_filename_regex.map(|s| Regex::new(s).unwrap());
        let exports = value.data.into_iter().map(|v| Self::from_export(v, re.as_ref()));

        let mut combined = CodeCovJsonExport::default();

        // combine
        for export in exports {
            for (filename, coverage) in export.coverage {
                let combined = combined.coverage.entry(filename).or_default();
                for (line, coverage) in coverage.0 {
                    let combined = combined
                        .0
                        .entry(line)
                        .or_insert_with(|| CodeCovCoverage { count: 0, covered: 0 });
                    combined.count += coverage.count;
                    combined.covered += coverage.covered;
                }
            }
        }

        combined
    }
}

/// A source line that is missed in a specific live instantiation but covered
/// by some other function in the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerInstantiationLine {
    pub line: u64,
    /// Mangled function names responsible for the missed line. The report layer
    /// formats them via `rustc_demangle`.
    pub function_names: Vec<String>,
}

/// Per-file uncovered lines, split into whole-file and per-instantiation cases
/// to mirror `llvm-cov report`'s file-summary accounting across InstantiationGroups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UncoveredFile {
    /// Lines for which no function in the file has any covered region.
    pub whole_file_missed: Vec<u64>,
    /// Lines covered by some function in the file but missed in one or more
    /// specific live instantiations (or in a dead instantiation that has no
    /// live sibling in its group covering the line).
    pub per_instantiation_missed: Vec<PerInstantiationLine>,
}

/// Files -> uncovered info.
pub type UncoveredLines = BTreeMap<String, UncoveredFile>;

#[non_exhaustive]
#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug))]
pub enum CoverageKind {
    Functions,
    Lines,
    Regions,
}

impl CoverageKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Functions => "functions",
            Self::Lines => "lines",
            Self::Regions => "regions",
        }
    }
}

impl LlvmCovJsonExport {
    pub fn demangle(&mut self) {
        for data in &mut self.data {
            if let Some(functions) = &mut data.functions {
                for func in functions {
                    func.name = format!("{:#}", rustc_demangle::demangle(&func.name));
                }
            }
        }
    }

    pub fn inject(&mut self, manifest_path: Utf8PathBuf) {
        self.cargo_llvm_cov = Some(CargoLlvmCov {
            version: env!("CARGO_PKG_VERSION"),
            manifest_path: manifest_path.into_string(),
        });
    }

    /// Gets the minimal lines coverage of all files.
    pub fn get_coverage_percent(&self, kind: CoverageKind) -> Result<f64> {
        let mut count = 0_f64;
        let mut covered = 0_f64;
        for data in &self.data {
            let totals = &data.totals.as_object().context("totals is not an object")?;
            let lines =
                &totals[kind.as_str()].as_object().context(format!("no {}", kind.as_str()))?;
            count += lines["count"].as_f64().context("no count")?;
            covered += lines["covered"].as_f64().context("no covered")?;
        }

        if count == 0_f64 {
            return Ok(0_f64);
        }

        Ok(covered * 100_f64 / count)
    }

    // Checks if each file meets the minimum line coverage threshold.
    #[must_use]
    pub fn all_files_above_coverage(&self, threshold: f64) -> bool {
        self.data
            .iter()
            .flat_map(|export| export.files.iter())
            .all(|file| file.summary.lines.percent > threshold)
    }

    /// Gets the list of uncovered lines of all files.
    ///
    /// Mirrors `llvm-cov report`'s file-summary line accounting, which is computed
    /// per InstantiationGroup (functions sharing a source location, e.g. multiple
    /// instantiations of one generic) and then summed across groups. See
    /// `llvm/tools/llvm-cov/CoverageSummaryInfo.cpp`
    /// `sumRegions` and `FunctionCoverageSummary::get(Group, Summaries)`, and
    /// `llvm/lib/ProfileData/Coverage/CoverageMapping.cpp` `FunctionInstantiationSetCollector`.
    ///
    /// Rule, per function F (only `kind == 0` Code regions are considered, matching
    /// LLVM's `sumRegions` filter):
    /// - `func_lines(F)` = lines touched by some Code region in F.
    /// - `func_covered(F)` = lines L where some Code region in F has `count > 0`.
    /// - `func_missed(F)` = `func_lines(F) - func_covered(F)`.
    /// - Group F into `(file, (first_region.line_start, first_region.column_start))`.
    /// - For each L in `func_missed(F)`:
    ///   - If F is live (`F.count > 0`): always emit `(L, F.name)`.
    ///   - If F is dead (`F.count == 0`): emit `(L, F.name)` only if no live function
    ///     in the same group has a covering region at L.
    ///
    /// Output partition: a missed line L is "whole-file" if no function anywhere in
    /// the file covers L; otherwise it is "per-instantiation" (the file is covered
    /// elsewhere, but specific instantiations contribute the miss).
    #[must_use]
    pub fn get_uncovered_lines(&self, ignore_filename_regex: Option<&str>) -> UncoveredLines {
        let re = ignore_filename_regex.map(|s| Regex::new(s).unwrap());

        // (file, group_key) -> functions in that InstantiationGroup. group_key =
        // (first_region.line_start, first_region.column_start), matching LLVM's
        // FunctionInstantiationSetCollector (CoverageMapping.cpp:1149).
        let mut groups: BTreeMap<(String, (u64, u64)), Vec<GroupedFunction>> = BTreeMap::new();
        let mut covered_anywhere: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();

        for data in &self.data {
            let Some(ref functions) = data.functions else { continue };
            for function in functions {
                if function.filenames.is_empty() || function.regions.is_empty() {
                    continue;
                }
                let file_name = &function.filenames[0];
                if let Some(ref re) = re {
                    if re.is_match(file_name) {
                        continue;
                    }
                }

                let first = &function.regions[0];
                let group_key = (first.line_start(), first.column_start());

                let mut lines = BTreeSet::new();
                let mut covered = BTreeSet::new();
                for region in &function.regions {
                    if region.kind() != 0 {
                        continue;
                    }
                    for line in RegionLocation::from(region).lines() {
                        lines.insert(line);
                        if region.execution_count() > 0 {
                            covered.insert(line);
                        }
                    }
                }
                if lines.is_empty() {
                    continue;
                }

                covered_anywhere.entry(file_name.clone()).or_default().extend(&covered);

                groups.entry((file_name.clone(), group_key)).or_default().push(GroupedFunction {
                    count: function.count,
                    name: function.name.clone(),
                    lines,
                    covered,
                });
            }
        }

        // file -> line -> responsible mangled function names.
        let mut all_missed: BTreeMap<String, BTreeMap<u64, Vec<String>>> = BTreeMap::new();

        for ((file_name, _group_key), instances) in &groups {
            let mut group_live_covered: BTreeSet<u64> = BTreeSet::new();
            for f in instances {
                if f.count > 0 {
                    group_live_covered.extend(&f.covered);
                }
            }

            for f in instances {
                for line in f.lines.difference(&f.covered) {
                    let suppressed = f.count == 0 && group_live_covered.contains(line);
                    if !suppressed {
                        all_missed
                            .entry(file_name.clone())
                            .or_default()
                            .entry(*line)
                            .or_default()
                            .push(f.name.clone());
                    }
                }
            }
        }

        let mut result: UncoveredLines = BTreeMap::new();
        for (file_name, line_map) in all_missed {
            let file_covered = covered_anywhere.get(&file_name);
            let mut uncovered_file = UncoveredFile::default();
            for (line, mut names) in line_map {
                let covered_elsewhere = file_covered.is_some_and(|c| c.contains(&line));
                if covered_elsewhere {
                    names.sort();
                    names.dedup();
                    uncovered_file
                        .per_instantiation_missed
                        .push(PerInstantiationLine { line, function_names: names });
                } else {
                    uncovered_file.whole_file_missed.push(line);
                }
            }
            if !uncovered_file.whole_file_missed.is_empty()
                || !uncovered_file.per_instantiation_missed.is_empty()
            {
                result.insert(file_name, uncovered_file);
            }
        }

        result
    }

    pub fn count_uncovered_functions(&self) -> Result<u64> {
        let mut count = 0_u64;
        let mut covered = 0_u64;
        for data in &self.data {
            let totals = &data.totals.as_object().context("totals is not an object")?;
            let functions = &totals["functions"].as_object().context("no functions")?;
            count += functions["count"].as_u64().context("no count")?;
            covered += functions["covered"].as_u64().context("no covered")?;
        }
        Ok(count.saturating_sub(covered))
    }

    pub fn count_uncovered_lines(&self) -> Result<u64> {
        let mut count = 0_u64;
        let mut covered = 0_u64;
        for data in &self.data {
            let totals = &data.totals.as_object().context("totals is not an object")?;
            let lines = &totals["lines"].as_object().context("no lines")?;
            count += lines["count"].as_u64().context("no count")?;
            covered += lines["covered"].as_u64().context("no covered")?;
        }
        Ok(count.saturating_sub(covered))
    }

    pub fn count_uncovered_regions(&self) -> Result<u64> {
        let mut count = 0_u64;
        let mut covered = 0_u64;
        for data in &self.data {
            let totals = &data.totals.as_object().context("totals is not an object")?;
            let regions = &totals["regions"].as_object().context("no regions")?;
            count += regions["count"].as_u64().context("no count")?;
            covered += regions["covered"].as_u64().context("no covered")?;
        }
        Ok(count.saturating_sub(covered))
    }
}

/// Json representation of one `CoverageMapping`
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Export {
    /// List of objects describing coverage for files
    pub files: Vec<File>,
    /// List of objects describing coverage for functions
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    functions: Option<Vec<Function>>,
    totals: serde_json::Value,
}

/// Coverage for a single file
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct File {
    /// List of Branches in the file
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    branches: Option<Vec<serde_json::Value>>,
    /// List of MC/DC records contained in the file
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    mcdc_records: Option<Vec<serde_json::Value>>,
    /// List of expansion records
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    expansions: Option<Vec<serde_json::Value>>,
    pub filename: String,
    /// List of Segments contained in the file
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    segments: Option<Vec<Segment>>,
    /// Object summarizing the coverage for this file
    summary: Summary,
}

/// Describes a segment of the file with a counter
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
struct Segment(
    /* Line */ u64,
    /* Col */ u64,
    /* Count */ u64,
    /* HasCount */ bool,
    /* IsRegionEntry */ bool,
    /* IsGapRegion */ bool,
);

impl Segment {
    fn line(&self) -> u64 {
        self.0
    }
    fn col(&self) -> u64 {
        self.1
    }
    fn count(&self) -> u64 {
        self.2
    }
    fn has_count(&self) -> bool {
        self.3
    }
    fn is_region_entry(&self) -> bool {
        self.4
    }
    fn is_gap_region(&self) -> bool {
        self.5
    }
}

impl fmt::Debug for Segment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Segment")
            .field("line", &self.line())
            .field("col", &self.col())
            .field("count", &self.count())
            .field("has_count", &self.has_count())
            .field("is_region_entry", &self.is_region_entry())
            .field("is_gap_region", &self.is_gap_region())
            .finish()
    }
}

/// Coverage info for a single function
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
struct Function {
    branches: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcdc_records: Option<Vec<serde_json::Value>>,
    count: u64,
    /// List of filenames that the function relates to
    filenames: Vec<String>,
    name: String,
    regions: Vec<Region>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
struct Region(
    /* LineStart */ u64,
    /* ColumnStart */ u64,
    /* LineEnd */ u64,
    /* ColumnEnd */ u64,
    /* ExecutionCount */ u64,
    /* FileID */ u64,
    /* ExpandedFileID */ u64,
    /* Kind */ u64,
);

impl Region {
    fn line_start(&self) -> u64 {
        self.0
    }
    fn column_start(&self) -> u64 {
        self.1
    }
    fn line_end(&self) -> u64 {
        self.2
    }
    fn column_end(&self) -> u64 {
        self.3
    }
    fn execution_count(&self) -> u64 {
        self.4
    }
    fn file_id(&self) -> u64 {
        self.5
    }
    fn expanded_file_id(&self) -> u64 {
        self.6
    }
    fn kind(&self) -> u64 {
        self.7
    }
}

impl fmt::Debug for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Region")
            .field("line_start", &self.line_start())
            .field("column_start", &self.column_start())
            .field("line_end", &self.line_end())
            .field("column_end", &self.column_end())
            .field("execution_count", &self.execution_count())
            .field("file_id", &self.file_id())
            .field("expanded_file_id", &self.expanded_file_id())
            .field("kind", &self.kind())
            .finish()
    }
}

/// The location of a region
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
struct RegionLocation {
    start_line: u64,
    end_line: u64,
    start_column: u64,
    end_column: u64,
}

impl From<&Region> for RegionLocation {
    fn from(region: &Region) -> Self {
        Self {
            start_line: region.line_start(),
            end_line: region.line_end(),
            start_column: region.column_start(),
            end_column: region.column_end(),
        }
    }
}

impl RegionLocation {
    fn lines(&self) -> impl Iterator<Item = u64> {
        self.start_line..=self.end_line
    }
}

/// One function's contribution to its InstantiationGroup, with kind=0 region
/// data pre-expanded into per-line sets.
struct GroupedFunction {
    count: u64,
    name: String,
    lines: BTreeSet<u64>,
    covered: BTreeSet<u64>,
}

/// Object summarizing the coverage for this file
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
struct Summary {
    /// Object summarizing branch coverage
    branches: CoverageCounts,
    /// Object summarizing mcdc coverage
    #[serde(skip_serializing_if = "Option::is_none")]
    mcdc: Option<CoverageCounts>,
    /// Object summarizing function coverage
    functions: CoverageCounts,
    instantiations: CoverageCounts,
    /// Object summarizing line coverage
    lines: CoverageCounts,
    /// Object summarizing region coverage
    regions: CoverageCounts,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
struct CoverageCounts {
    count: u64,
    covered: u64,
    // Currently only branches and regions has this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    notcovered: Option<u64>,
    percent: f64,
}

/// Information that is not part of the llvm-cov JSON export, but instead injected afterwards by us.
#[derive(Debug, Default, Serialize)]
#[cfg_attr(test, derive(PartialEq))]
struct CargoLlvmCov {
    /// Version of this project, which allows projects that depend on it, to express and verify
    /// requirements on specific versions.
    version: &'static str,
    /// Resolved path to the `Cargo.toml` manifest.
    manifest_path: String,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fs_err as fs;

    use super::*;

    #[test]
    fn test_parse_llvm_cov_json() {
        let files: Vec<_> = glob::glob(&format!(
            "{}/tests/fixtures/coverage-reports/**/*.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|path| !path.to_str().unwrap().contains("codecov.json"))
        .collect();
        assert!(!files.is_empty());

        for file in files {
            let s = fs::read_to_string(file).unwrap();
            let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();
            assert_eq!(json.type_, "llvm.coverage.json.export");
            assert_eq!(json.version, "3.1.0");
            assert_eq!(json.cargo_llvm_cov, None);
            serde_json::to_string(&json).unwrap();
        }
    }

    fn test_get_coverage_percent(kind: CoverageKind) {
        let expected = match kind {
            CoverageKind::Functions => 100_f64,
            CoverageKind::Lines => 57.142_857_142_857_146,
            CoverageKind::Regions => 61.538_461_538_461_54,
        };

        // There are 5 different percentages, make sure we pick the correct one.
        let file = format!(
            "{}/tests/fixtures/coverage-reports/no_coverage/no_coverage.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        let actual = json.get_coverage_percent(kind).unwrap();
        assert_eq!(actual, expected, "kind={kind:?},actual={actual}");
    }

    #[test]
    fn test_get_functions_percent() {
        test_get_coverage_percent(CoverageKind::Functions);
    }

    #[test]
    fn test_get_lines_percent() {
        test_get_coverage_percent(CoverageKind::Lines);
    }

    #[test]
    fn test_get_regions_percent() {
        test_get_coverage_percent(CoverageKind::Regions);
    }

    #[test]
    fn test_all_files_above_coverage() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

        let cases = vec![
            // (path, minimum_coverage, all_files_above_coverage)
            ("tests/fixtures/coverage-reports/no_coverage/no_coverage.json", 60_f64, false),
            ("tests/fixtures/coverage-reports/no_coverage/no_coverage.json", 50_f64, true),
            ("tests/fixtures/coverage-reports/no_test/no_test.json", 90_f64, false),
        ];

        for (file, min_coverage, covered) in cases {
            let file = &manifest_dir.join(file);
            let s = fs::read_to_string(file).unwrap();
            let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

            assert_eq!(json.all_files_above_coverage(min_coverage), covered, "{file:?}");
        }
    }

    #[test]
    fn test_count_uncovered() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

        let cases = &[
            // (path, uncovered_functions, uncovered_lines, uncovered_regions)
            ("tests/fixtures/coverage-reports/no_coverage/no_coverage.json", 0, 6, 5),
            ("tests/fixtures/coverage-reports/no_test/no_test.json", 1, 7, 7),
        ];

        for &(file, uncovered_functions, uncovered_lines, uncovered_regions) in cases {
            let file = &manifest_dir.join(file);
            let s = fs::read_to_string(file).unwrap();
            let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();
            assert_eq!(json.count_uncovered_functions().unwrap(), uncovered_functions, "{file:?}");
            assert_eq!(json.count_uncovered_lines().unwrap(), uncovered_lines, "{file:?}");
            assert_eq!(json.count_uncovered_regions().unwrap(), uncovered_regions, "{file:?}");
        }
    }

    #[test]
    fn test_get_uncovered_lines() {
        // Given a coverage report which includes function regions:
        // There are 5 different percentages, make sure we pick the correct one.
        let file = format!("{}/tests/fixtures/show-missing-lines.json", env!("CARGO_MANIFEST_DIR"));
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        // When finding uncovered lines in that report:
        let ignore_filename_regex = None;
        let uncovered_lines = json.get_uncovered_lines(ignore_filename_regex);

        // Then make sure the file / line data matches the `llvm-cov report` output:
        let expected: UncoveredLines = vec![("src/lib.rs".to_owned(), UncoveredFile {
            whole_file_missed: vec![7, 8, 9],
            per_instantiation_missed: vec![],
        })]
        .into_iter()
        .collect();
        assert_eq!(uncovered_lines, expected);
    }

    #[test]
    /// This was a case when counting line coverage based on the segments in files lead to
    /// incorrect results but doing it based on regions inside functions (the way `llvm-cov
    /// report`) leads to complete line coverage.
    fn test_get_uncovered_lines_complete() {
        let file = format!(
            "{}/tests/fixtures/show-missing-lines-complete.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        let ignore_filename_regex = None;
        let uncovered_lines = json.get_uncovered_lines(ignore_filename_regex);

        let expected: UncoveredLines = UncoveredLines::new();
        assert_eq!(uncovered_lines, expected);
    }

    #[test]
    fn test_get_uncovered_lines_multi_missing() {
        // Given a coverage report which includes a line with multiple functions via macros + two
        // other uncovered lines:
        let file = format!(
            "{}/tests/fixtures/show-missing-lines-multi-missing.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        // When finding uncovered lines in that report:
        let ignore_filename_regex = None;
        let uncovered_lines = json.get_uncovered_lines(ignore_filename_regex);

        // Then make sure the file / line data matches the `llvm-cov report` output:
        let expected: UncoveredLines = vec![("src/lib.rs".to_owned(), UncoveredFile {
            whole_file_missed: vec![15, 17],
            per_instantiation_missed: vec![],
        })]
        .into_iter()
        .collect();
        // This was just '11', i.e. there were two problems:
        // 1) line 11 has a serde macro which expands to multiple functions; some of those were
        //    covered, which should be presented as a "covered" 11th line.
        // 2) only the last function with missing lines were reported, so 15 and 17 was missing.
        // The group-aware rule preserves this: dead instances at line 11 are grouped with
        // the live RelationDict at the same (line, col) and suppressed by it.
        assert_eq!(uncovered_lines, expected);
    }

    /// Same-group asymmetric live instantiations: two live instantiations of one generic
    /// share a source location and so belong to the same InstantiationGroup. One has a
    /// `kind=0 count=0` region at line 289 with no covering region in the same function; the
    /// other covers line 289 with count=1. The per-line max view shows the line covered, but
    /// the file summary counts it as missed because LLVM's intra-group MAX-per-dimension merge
    /// surfaces the asymmetry.
    #[test]
    fn test_get_uncovered_lines_same_group_asymmetric() {
        let file = format!(
            "{}/tests/fixtures/show-missing-lines-same-group-asymmetric.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        let uncovered_lines = json.get_uncovered_lines(None);

        let expected: UncoveredLines = vec![("src/lib.rs".to_owned(), UncoveredFile {
            whole_file_missed: vec![],
            per_instantiation_missed: vec![PerInstantiationLine {
                line: 289,
                function_names: vec!["_RNvCsTEST1_5crate1_14list_recursive".to_owned()],
            }],
        })]
        .into_iter()
        .collect();
        assert_eq!(uncovered_lines, expected);
    }

    /// Different-group dead vs live (the strum #404 case): a dead `Display::fmt` and a live
    /// `EnumIter::iter` are in different InstantiationGroups (different source locations).
    /// The file summary sums per-group totals so Display's miss stands, even though line 3 is
    /// covered by EnumIter from a different group.
    #[test]
    fn test_get_uncovered_lines_different_group() {
        let file = format!(
            "{}/tests/fixtures/show-missing-lines-different-group.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        let uncovered_lines = json.get_uncovered_lines(None);

        let expected: UncoveredLines = vec![("src/lib.rs".to_owned(), UncoveredFile {
            whole_file_missed: vec![],
            per_instantiation_missed: vec![PerInstantiationLine {
                line: 3,
                function_names: vec!["_RNvXNtCsTEST_4test3Foo7Display3fmt".to_owned()],
            }],
        })]
        .into_iter()
        .collect();
        assert_eq!(uncovered_lines, expected);
    }

    /// Intra-group suppression: a dead instance and a live instance share a source location.
    /// The live sibling covers the line, so the dead instance's miss is forgiven (matches
    /// LLVM's intra-group merge that lets a single fully-covering instance hide the miss).
    #[test]
    fn test_get_uncovered_lines_intra_group_suppression() {
        let file = format!(
            "{}/tests/fixtures/show-missing-lines-intra-group-suppression.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        let uncovered_lines = json.get_uncovered_lines(None);

        let expected: UncoveredLines = UncoveredLines::new();
        assert_eq!(uncovered_lines, expected);
    }
}

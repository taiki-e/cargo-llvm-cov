use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

// https://github.com/llvm/llvm-project/blob/llvmorg-15.0.0/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L13-L47
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct LlvmCovJsonExport {
    /// List of one or more export objects
    pub(crate) data: Vec<Export>,
    // llvm.coverage.json.export
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) version: String,
}

/// Files -> list of uncovered lines.
pub(crate) type UncoveredLines = BTreeMap<String, Vec<u64>>;

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

    #[must_use]
    pub fn function_names(&self, filter: impl Fn(&str) -> bool) -> Vec<&str> {
        let mut v = vec![];
        for data in &self.data {
            if let Some(functions) = data.functions.as_ref().filter(|f| !f.is_empty()) {
                v.reserve(functions.len() / 2);
                for func in functions {
                    if filter(&func.name) {
                        v.push(&*func.name);
                    }
                }
            }
        }
        v
    }

    /// Gets the minimal lines coverage of all files.
    pub fn get_lines_percent(&self) -> Result<f64> {
        let mut count = 0_f64;
        let mut covered = 0_f64;
        for data in &self.data {
            let totals = &data.totals.as_object().context("totals is not an object")?;
            let lines = &totals["lines"].as_object().context("no lines")?;
            count += lines["count"].as_f64().context("no count")?;
            covered += lines["covered"].as_f64().context("no covered")?;
        }

        if count == 0_f64 {
            return Ok(0_f64);
        }

        Ok(covered * 100_f64 / count)
    }

    /// Gets the list of uncovered lines of all files.
    #[must_use]
    pub fn get_uncovered_lines(&self, ignore_filename_regex: &Option<String>) -> UncoveredLines {
        let mut uncovered_files: UncoveredLines = BTreeMap::new();
        let mut covered_files: UncoveredLines = BTreeMap::new();
        let mut re: Option<regex::Regex> = None;
        if let Some(ref ignore_filename_regex) = *ignore_filename_regex {
            re = Some(regex::Regex::new(ignore_filename_regex).unwrap());
        }
        for data in &self.data {
            if let Some(ref functions) = data.functions {
                // Iterate over all functions inside the coverage data.
                for function in functions {
                    if function.filenames.is_empty() {
                        continue;
                    }
                    let file_name = &function.filenames[0];
                    if let Some(ref re) = re {
                        if re.is_match(file_name) {
                            continue;
                        }
                    }
                    let mut lines: BTreeMap<u64, u64> = BTreeMap::new();
                    // Iterate over all possible regions inside a function:
                    for region in &function.regions {
                        // LineStart, ColumnStart, LineEnd, ColumnEnd, ExecutionCount, FileID, ExpandedFileID, Kind
                        let line_start = region.0;
                        let line_end = region.2;
                        let exec_count = region.4;
                        // Remember the execution count for each line of that region:
                        for line in line_start..=line_end {
                            *lines.entry(line).or_insert(0) += exec_count;
                        }
                    }

                    let mut uncovered_lines: Vec<u64> = lines
                        .iter()
                        .filter(|(_line, exec_count)| **exec_count == 0)
                        .map(|(line, _exec_count)| *line)
                        .collect();
                    let mut covered_lines: Vec<u64> = lines
                        .iter()
                        .filter(|(_line, exec_count)| **exec_count > 0)
                        .map(|(line, _exec_count)| *line)
                        .collect();
                    if !uncovered_lines.is_empty() {
                        uncovered_files
                            .entry(file_name.clone())
                            .or_insert_with(Vec::new)
                            .append(&mut uncovered_lines);
                    }
                    if !covered_lines.is_empty() {
                        covered_files
                            .entry(file_name.clone())
                            .or_insert_with(Vec::new)
                            .append(&mut covered_lines);
                    }
                }
            }
        }

        for uncovered_file in &mut uncovered_files {
            // Check if a line is both covered and non-covered. It's covered in this case.
            let file_name = uncovered_file.0;
            let uncovered_lines = uncovered_file.1;
            if let Some(covered_lines) = covered_files.get(file_name) {
                uncovered_lines.retain(|&x| !covered_lines.contains(&x));
            }

            // Remove duplicates.
            uncovered_lines.sort_unstable();
            uncovered_lines.dedup();
        }

        // Remove empty keys.
        uncovered_files.retain(|_, v| !v.is_empty());

        uncovered_files
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
pub(crate) struct Export {
    /// List of objects describing coverage for files
    pub(crate) files: Vec<File>,
    /// List of objects describing coverage for functions
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) functions: Option<Vec<Function>>,
    pub(crate) totals: serde_json::Value,
}

/// Coverage for a single file
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(crate) struct File {
    /// List of Branches in the file
    ///
    /// This is None if report is summary-only.
    // https://github.com/llvm/llvm-project/blob/llvmorg-15.0.0/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L93
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) branches: Option<Vec<serde_json::Value>>,
    /// List of expansion records
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expansions: Option<Vec<serde_json::Value>>,
    pub(crate) filename: String,
    /// List of Segments contained in the file
    ///
    /// This is None if report is summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) segments: Option<Vec<Segment>>,
    /// Object summarizing the coverage for this file
    pub(crate) summary: Summary,
}

/// Describes a segment of the file with a counter
// https://github.com/llvm/llvm-project/blob/llvmorg-15.0.0/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L80
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(crate) struct Segment(
    /* Line */ pub(crate) u64,
    /* Col */ pub(crate) u64,
    /* Count */ pub(crate) u64,
    /* HasCount */ pub(crate) bool,
    /* IsRegionEntry */ pub(crate) bool,
    /* IsGapRegion */ pub(crate) bool,
);

// https://github.com/llvm/llvm-project/blob/llvmorg-15.0.0/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L259
/// Coverage info for a single function
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(crate) struct Function {
    pub(crate) branches: Vec<serde_json::Value>,
    pub(crate) count: u64,
    /// List of filenames that the function relates to
    pub(crate) filenames: Vec<String>,
    pub(crate) name: String,
    pub(crate) regions: Vec<Region>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(crate) struct Region(
    /* LineStart */ pub(crate) u64,
    /* ColumnStart */ pub(crate) u64,
    /* LineEnd */ pub(crate) u64,
    /* ColumnEnd */ pub(crate) u64,
    /* ExecutionCount */ pub(crate) u64,
    /* FileID */ pub(crate) u64,
    /* ExpandedFileID */ pub(crate) u64,
    /* Kind */ pub(crate) u64,
);

/// Object summarizing the coverage for this file
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(crate) struct Summary {
    /// Object summarizing branch coverage
    pub(crate) branches: CoverageCounts,
    /// Object summarizing function coverage
    pub(crate) functions: CoverageCounts,
    pub(crate) instantiations: CoverageCounts,
    /// Object summarizing line coverage
    pub(crate) lines: CoverageCounts,
    /// Object summarizing region coverage
    pub(crate) regions: CoverageCounts,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub(crate) struct CoverageCounts {
    pub(crate) count: u64,
    pub(crate) covered: u64,
    // Currently only branches and regions has this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notcovered: Option<u64>,
    pub(crate) percent: f64,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fs_err as fs;

    use super::*;

    #[test]
    fn parse_llvm_cov_json() {
        let files: Vec<_> = glob::glob(&format!(
            "{}/tests/fixtures/coverage-reports/**/*.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
        assert!(!files.is_empty());

        for file in files {
            let s = fs::read_to_string(file).unwrap();
            let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();
            assert_eq!(json.type_, "llvm.coverage.json.export");
            assert!(json.version.starts_with("2.0."));
            serde_json::to_string(&json).unwrap();
        }
    }

    #[test]
    fn test_get_lines_percent() {
        // There are 5 different percentages, make sure we pick the correct one.
        let file = format!(
            "{}/tests/fixtures/coverage-reports/no_coverage/no_coverage.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = fs::read_to_string(file).unwrap();
        let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();

        let percent = json.get_lines_percent().unwrap();

        let error_margin = f64::EPSILON;
        assert!((percent - 69.565_217_391_304_34).abs() < error_margin);
    }

    #[test]
    fn test_count_uncovered() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

        let cases = &[
            // (path, uncovered_functions, uncovered_lines, uncovered_regions)
            ("tests/fixtures/coverage-reports/no_coverage/no_coverage.json", 0, 7, 6),
            ("tests/fixtures/coverage-reports/no_test/no_test.json", 1, 7, 6),
        ];

        for &(file, uncovered_functions, uncovered_lines, uncovered_regions) in cases {
            let file = manifest_dir.join(file);
            let s = fs::read_to_string(file).unwrap();
            let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();
            assert_eq!(json.count_uncovered_functions().unwrap(), uncovered_functions);
            assert_eq!(json.count_uncovered_lines().unwrap(), uncovered_lines);
            assert_eq!(json.count_uncovered_regions().unwrap(), uncovered_regions);
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
        let uncovered_lines = json.get_uncovered_lines(&ignore_filename_regex);

        // Then make sure the file / line data matches the `llvm-cov report` output:
        let expected: UncoveredLines =
            vec![("src/lib.rs".to_string(), vec![7, 8, 9])].into_iter().collect();
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
        let uncovered_lines = json.get_uncovered_lines(&ignore_filename_regex);

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
        let uncovered_lines = json.get_uncovered_lines(&ignore_filename_regex);

        // Then make sure the file / line data matches the `llvm-cov report` output:
        let expected: UncoveredLines =
            vec![("src/lib.rs".to_string(), vec![15, 17])].into_iter().collect();
        // This was just '11', i.e. there were two problems:
        // 1) line 11 has a serde macro which expands to multiple functions; some of those were
        //    covered, which should be presented as a "covered" 11th line.
        // 2) only the last function with missing lines were reported, so 15 and 17 was missing.
        assert_eq!(uncovered_lines, expected);
    }
}

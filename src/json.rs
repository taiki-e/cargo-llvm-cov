use serde::{Deserialize, Serialize};

// https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L13-L47
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[allow(unreachable_pub)]
pub struct LlvmCovJsonExport {
    /// List of one or more export objects
    pub(crate) data: Vec<Export>,
    // llvm.coverage.json.export
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) version: String,
}

impl LlvmCovJsonExport {
    #[allow(unreachable_pub, dead_code)]
    pub fn demangle(&mut self) {
        for data in &mut self.data {
            if let Some(functions) = &mut data.functions {
                for func in functions {
                    func.name = format!("{:#}", rustc_demangle::demangle(&func.name));
                }
            }
        }
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
    // https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L93
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
// https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L80
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

// https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L259
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
    use fs_err as fs;

    use super::LlvmCovJsonExport;

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
}

use serde::{Deserialize, Serialize};

// https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L13-L47
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct LlvmCovJsonExport {
    /// List of one or more export objects
    pub data: Vec<Export>,
    // llvm.coverage.json.export
    #[serde(rename = "type")]
    pub type_: String,
    pub version: String,
}

impl LlvmCovJsonExport {
    pub fn demangle(&mut self) {
        for data in &mut self.data {
            for func in &mut data.functions {
                func.name = format!("{:#}", rustc_demangle::demangle(&func.name));
            }
        }
    }
}

/// Json representation of one CoverageMapping
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Export {
    /// List of objects describing coverage for files
    pub files: Vec<File>,
    /// List of objects describing coverage for functions
    pub functions: Vec<Function>,
    #[cfg(test)]
    pub totals: serde_json::Value,
}

/// Coverage for a single file
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct File {
    /// List of Branches in the file
    // https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L93
    #[cfg(test)]
    pub branches: Vec<serde_json::Value>,
    /// List of expansion records
    #[cfg(test)]
    pub expansions: Vec<serde_json::Value>,
    pub filename: String,
    /// List of Segments contained in the file
    pub segments: Vec<Segment>,
    /// Object summarizing the coverage for this file
    pub summary: Summary,
}

/// Describes a segment of the file with a counter
// https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L80
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Segment(
    /* Line */ pub u64,
    /* Col */ pub u64,
    /* Count */ pub u64,
    /* HasCount */ pub bool,
    /* IsRegionEntry */ pub bool,
    /* IsGapRegion */ pub bool,
);

// https://github.com/llvm/llvm-project/blob/c0db8d50ca3ceb1301b2ade2fb86c591a5b64e5c/llvm/tools/llvm-cov/CoverageExporterJson.cpp#L259
/// Coverage info for a single function
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Function {
    #[cfg(test)]
    pub branches: Vec<serde_json::Value>,
    pub count: u64,
    /// List of filenames that the function relates to
    pub filenames: Vec<String>,
    pub name: String,
    pub regions: Vec<Region>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Region(
    /* LineStart */ pub u64,
    /* ColumnStart */ pub u64,
    /* LineEnd */ pub u64,
    /* ColumnEnd */ pub u64,
    /* ExecutionCount */ pub u64,
    /* FileID */ pub u64,
    /* ExpandedFileID */ pub u64,
    /* Kind */ pub u64,
);

/// Object summarizing the coverage for this file
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Summary {
    /// Object summarizing branch coverage
    pub branches: CoverageCounts,
    /// Object summarizing function coverage
    pub functions: CoverageCounts,
    pub instantiations: CoverageCounts,
    /// Object summarizing line coverage
    pub lines: CoverageCounts,
    /// Object summarizing region coverage
    pub regions: CoverageCounts,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct CoverageCounts {
    pub count: u64,
    pub covered: u64,
    // Currently only branches and regions has this field.
    pub notcovered: Option<u64>,
    pub percent: f64,
}

mod tests {
    use super::{super::fs, LlvmCovJsonExport};

    #[test]
    fn parse_llvm_cov_json_full() {
        let files: Vec<_> = glob::glob(&format!(
            "{}/tests/fixtures/coverage-reports/**/*.full.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
        assert_eq!(files.len(), 11);

        for file in files {
            let s = fs::read_to_string(file).unwrap();
            let json = serde_json::from_str::<LlvmCovJsonExport>(&s).unwrap();
            assert_eq!(json.type_, "llvm.coverage.json.export");
            assert!(json.version.starts_with("2.0."));
            serde_json::to_string(&json).unwrap();
        }
    }
}

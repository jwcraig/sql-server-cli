use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::CsvMultiResultNaming;
use crate::db::types::ResultSet;

pub fn write_result_sets(
    base_path: &Path,
    result_sets: &[ResultSet],
    naming: CsvMultiResultNaming,
) -> Result<Vec<PathBuf>> {
    let multiple = result_sets.len() > 1;
    let mut paths = Vec::new();

    for (index, result_set) in result_sets.iter().enumerate() {
        let target = expand_csv_path(base_path, index + 1, multiple, naming);
        let mut writer = csv::Writer::from_path(&target)?;
        let headers = result_set
            .columns
            .iter()
            .map(|col| col.name.as_str())
            .collect::<Vec<_>>();
        writer.write_record(headers)?;
        for row in &result_set.rows {
            let record = row.iter().map(|value| value.as_csv()).collect::<Vec<_>>();
            writer.write_record(record)?;
        }
        writer.flush()?;
        paths.push(target);
    }

    Ok(paths)
}

fn expand_csv_path(
    base_path: &Path,
    index: usize,
    multiple: bool,
    naming: CsvMultiResultNaming,
) -> PathBuf {
    let base_str = base_path.to_string_lossy();
    if base_str.contains("{n}") {
        return PathBuf::from(base_str.replace("{n}", &index.to_string()));
    }

    if multiple && matches!(naming, CsvMultiResultNaming::SuffixNumber) {
        let stem = base_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("results");
        let ext = base_path.extension().and_then(|s| s.to_str());
        let mut filename = format!("{}-{}", stem, index);
        if let Some(ext) = ext {
            filename.push('.');
            filename.push_str(ext);
        }
        let mut path = base_path.to_path_buf();
        path.set_file_name(filename);
        return path;
    }

    base_path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::{Column, ResultSet, Value};
    use std::env;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let mut dir = env::temp_dir();
        dir.push(format!("sscli-csv-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_result_set() -> ResultSet {
        ResultSet {
            columns: vec![Column {
                name: "id".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Int(1)]],
        }
    }

    #[test]
    fn writes_multiple_csv_files_with_suffix() {
        let dir = temp_dir("suffix");
        let base = dir.join("results.csv");
        let result_sets = vec![sample_result_set(), sample_result_set()];

        let paths = write_result_sets(&base, &result_sets, CsvMultiResultNaming::SuffixNumber)
            .expect("write csv");

        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("results-1.csv"));
        assert!(paths[1].ends_with("results-2.csv"));
    }

    #[test]
    fn writes_csv_with_placeholder() {
        let dir = temp_dir("placeholder");
        let base = dir.join("results-{n}.csv");
        let result_sets = vec![sample_result_set(), sample_result_set()];

        let paths = write_result_sets(&base, &result_sets, CsvMultiResultNaming::Placeholder)
            .expect("write csv");

        assert!(paths[0].ends_with("results-1.csv"));
        assert!(paths[1].ends_with("results-2.csv"));
    }
}

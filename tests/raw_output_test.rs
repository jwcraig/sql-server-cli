use sscli::db::types::{Column, ResultSet, Value};
use sscli::output::raw::{RawFormat, RawOptions, render_result_sets};

fn sample_result_set() -> ResultSet {
    ResultSet {
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: None,
            },
            Column {
                name: "name".to_string(),
                data_type: None,
            },
            Column {
                name: "missing".to_string(),
                data_type: None,
            },
        ],
        rows: vec![
            vec![
                Value::Int(1234567),
                Value::Text("alpha".to_string()),
                Value::Null,
            ],
            vec![
                Value::Int(2),
                Value::Text("contains\ttab".to_string()),
                Value::Text("present".to_string()),
            ],
        ],
    }
}

#[test]
fn tsv_raw_output_is_data_only_without_display_formatting() {
    let options = RawOptions {
        format: RawFormat::Tsv,
        headers: false,
        null_value: "".to_string(),
        result_set: None,
        all_result_sets: false,
    };

    let output = render_result_sets(&[sample_result_set()], &options).expect("render tsv");

    assert_eq!(output, "1234567\talpha\t\n2\tcontains tab\tpresent\n");
}

#[test]
fn csv_raw_output_can_include_headers() {
    let options = RawOptions {
        format: RawFormat::Csv,
        headers: true,
        null_value: "".to_string(),
        result_set: None,
        all_result_sets: false,
    };

    let output = render_result_sets(&[sample_result_set()], &options).expect("render csv");

    assert_eq!(
        output,
        "id,name,missing\n1234567,alpha,\n2,contains\ttab,present\n"
    );
}

#[test]
fn jsonl_raw_output_emits_one_object_per_row() {
    let options = RawOptions {
        format: RawFormat::Jsonl,
        headers: true,
        null_value: "".to_string(),
        result_set: None,
        all_result_sets: false,
    };

    let output = render_result_sets(&[sample_result_set()], &options).expect("render jsonl");

    assert_eq!(
        output,
        "{\"id\":1234567,\"missing\":null,\"name\":\"alpha\"}\n{\"id\":2,\"missing\":\"present\",\"name\":\"contains\\ttab\"}\n"
    );
}

#[test]
fn raw_output_requires_result_set_selection_for_multiple_sets() {
    let options = RawOptions {
        format: RawFormat::Tsv,
        headers: true,
        null_value: "".to_string(),
        result_set: None,
        all_result_sets: false,
    };

    let err = render_result_sets(&[sample_result_set(), sample_result_set()], &options)
        .expect_err("multiple result sets should require selection");

    assert!(err.to_string().contains("--result-set"));
}

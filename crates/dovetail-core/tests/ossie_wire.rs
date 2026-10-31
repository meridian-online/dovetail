//! The Ossie wire types: reading and writing a model, against Brightfield's stand-in
//! fixture and the vendored schema.

use dovetail_core::ossie::{read, write, Role};

const FIXTURE: &str = include_str!("../../../tests/ossie-fixtures/brightfield-retail.ossie.yaml");
const SCHEMA: &str = include_str!("../../../vendor/ossie/ossie-schema.json");

fn validate(yaml: &str) {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    let doc: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&doc)
        .map(|e| format!("{} at {}", e, e.instance_path))
        .collect();
    assert!(errors.is_empty(), "schema errors: {errors:#?}");
}

#[test]
fn brightfield_fixture_reads_and_validates() {
    let m = read(FIXTURE).unwrap();
    assert_eq!(m.datasets.len(), 8);
    assert_eq!(m.relationships.len(), 7);
    let categories = m.datasets.iter().find(|d| d.name == "categories").unwrap();
    assert_eq!(categories.meta.role, Role::Reference);
    assert_eq!(categories.meta.folded_into.as_deref(), Some("products"));
    validate(FIXTURE);
}

#[test]
fn write_then_read_is_identity() {
    let m = read(FIXTURE).unwrap();
    let out = write(&m);
    validate(&out);
    assert_eq!(read(&out).unwrap(), m);
    assert_eq!(write(&read(&out).unwrap()), out);
}

#[test]
fn unknown_keys_survive() {
    let yaml = r#"version: 0.2.0.dev0
name: m
datasets:
- name: a
  source: a.csv
  unique_keys:
  - - id
  fields:
  - name: id
    expression:
      sql: id
    label: x
    custom_extensions:
    - vendor_name: MERIDIAN
      data: '{"decided_by":"dovetail","note":"y"}'
  custom_extensions:
  - vendor_name: OTHER
    data: '{"k":1}'
custom_extensions:
- vendor_name: MERIDIAN
  data: '{"generator":"g","skipped_sources":[{"path":"p.bin","reason":"binary","hint":"z"}]}'
"#;
    let m = read(yaml).unwrap();
    let out = write(&m);
    assert!(out.contains(r#""hint":"z""#), "{out}");
    assert!(out.contains("label: x"), "{out}");
    assert!(out.contains("unique_keys"), "{out}");
    assert!(out.contains("vendor_name: OTHER"), "{out}");
    assert!(out.contains(r#""note":"y""#), "{out}");
    assert_eq!(read(&out).unwrap(), m);
}

#[test]
fn key_order_is_the_wire_order() {
    let out = write(&read(FIXTURE).unwrap());
    let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
    let keys = |v: &serde_yaml::Value| -> Vec<String> {
        v.as_mapping()
            .unwrap()
            .keys()
            .map(|k| k.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        keys(&doc),
        [
            "version",
            "name",
            "datasets",
            "relationships",
            "custom_extensions"
        ]
    );
    let products = &doc["datasets"][2];
    assert_eq!(
        keys(products),
        [
            "name",
            "source",
            "primary_key",
            "fields",
            "custom_extensions"
        ]
    );
    assert_eq!(
        keys(&products["fields"][3]),
        ["name", "expression", "datatype", "custom_extensions"]
    );
    assert_eq!(
        keys(&doc["relationships"][0]),
        [
            "name",
            "from",
            "to",
            "from_columns",
            "to_columns",
            "custom_extensions"
        ]
    );
}

#[test]
fn a_model_without_the_meridian_extension_reads_with_defaults() {
    let yaml = "version: 0.2.0.dev0\nname: m\ndatasets:\n- name: a\n  source: a.csv\n  fields:\n  - name: id\n    expression:\n      sql: id\n";
    let m = read(yaml).unwrap();
    assert_eq!(m.datasets[0].meta.source_file, "a.csv");
    assert_eq!(m.datasets[0].meta.role, Role::Unclear);
    validate(&write(&m));
}

#[test]
fn new_keys_round_trip_in_wire_order() {
    let yaml = r#"version: 0.2.0.dev0
name: m
datasets:
- name: a
  source: a.csv
  custom_extensions:
  - vendor_name: MERIDIAN
    data: '{"role":"event","role_scores":{"entity":0.1,"event":0.9},"fold_column":"c","combined_into":"ab","combination":{"kind":"union","parts":["a","b"],"token_column":"part"},"excluded":false,"decided_by":"dovetail","source_file":"a.csv"}'
custom_extensions:
- vendor_name: MERIDIAN
  data: '{"generator":"g","pass":"sample","typing":{"source":"packages"},"sample_rows":50000}'
"#;
    let m = read(yaml).unwrap();
    assert_eq!(m.meta.sample_rows, Some(50_000));
    let out = write(&m);
    assert!(out.contains(r#""role":"event","role_scores":{"entity":0.1,"event":0.9},"fold_column":"c","combined_into":"ab","combination":"#), "{out}");
    assert!(
        out.contains(
            r#""generator":"g","pass":"sample","typing":{"source":"packages"},"sample_rows":50000"#
        ),
        "{out}"
    );
    assert_eq!(read(&out).unwrap(), m);
}

#[test]
fn a_rejected_list_with_unsafe_values_still_writes_and_reads() {
    let mut m = read(FIXTURE).unwrap();
    m.relationships[0].meta.evidence.value_overlap = f64::NAN;
    let mut rejected = m.relationships[1].clone();
    let odd: serde_yaml::Value = serde_yaml::from_str("{1: .nan, a: .inf}").unwrap();
    rejected.extras.insert("odd".to_string(), odd);
    m.meta.rejected.push(rejected);
    let out = write(&m);
    let back = read(&out).unwrap();
    assert_eq!(back.relationships[0].meta.evidence.value_overlap, 0.0);
    assert_eq!(back.meta.rejected.len(), 1);
    assert!(back.meta.rejected[0].extras.contains_key("odd"));
}

#[test]
fn an_empty_source_file_reads_back_empty() {
    let yaml = r#"version: 0.2.0.dev0
name: m
datasets:
- name: a
  source: raw_a
  fields:
  - name: id
    expression:
      sql: id
  custom_extensions:
  - vendor_name: MERIDIAN
    data: '{"source_file":""}'
- name: b
  source: b.csv
  fields:
  - name: id
    expression:
      sql: id
"#;
    let m = read(yaml).unwrap();
    assert_eq!(m.datasets[0].meta.source_file, "");
    // A dataset with no Meridian entry defaults to its source, as Brightfield reads it.
    assert_eq!(m.datasets[1].meta.source_file, "b.csv");
    assert_eq!(read(&write(&m)).unwrap().datasets[0].meta.source_file, "");
}

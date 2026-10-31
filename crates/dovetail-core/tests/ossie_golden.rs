//! The golden models: the retail fixture through `run_model` with its data packages,
//! as the sample pass and as the full pass verifying that sample, and the Meridian keys
//! Brightfield's reader takes from each.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dovetail_core::evidence::SAMPLE_ROWS;
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::ossie::{read, run_model, write, ModelRequest, Verified};
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;

const SCHEMA: &str = include_str!("../../../vendor/ossie/ossie-schema.json");
const SAMPLE_GOLDEN: &str = "../../tests/ossie-fixtures/retail.sample.ossie.yaml";
const FULL_GOLDEN: &str = "../../tests/ossie-fixtures/retail.full.ossie.yaml";

fn validate(yaml: &str) {
    let schema: Json = serde_json::from_str(SCHEMA).unwrap();
    let doc: Json = serde_yaml::from_str(yaml).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&doc)
        .map(|e| format!("{} at {}", e, e.instance_path))
        .collect();
    assert!(errors.is_empty(), "schema errors: {errors:#?}");
}

fn request(db: &Path, pass: Verified, verify: Option<PathBuf>) -> ModelRequest {
    ModelRequest {
        db: db.to_path_buf(),
        packages: Some(retail_packages_dir()),
        pass,
        base: None,
        verify,
        name: "retail".to_string(),
        typing: TypingRequest::Floor,
    }
}

/// The retail fixture's sample pass, then its full pass verifying that sample, each as
/// written.
fn passes() -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let (sample, warnings) = run_model(&request(&db, Verified::Sample, None)).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let sample = write(&sample);
    let path = dir.path().join("sample.ossie.yaml");
    std::fs::write(&path, &sample).unwrap();
    let (full, warnings) = run_model(&request(&db, Verified::Full, Some(path))).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    (sample, write(&full))
}

/// Check `yaml` against the golden at `golden`, which names the generator without its
/// version, so a release does not fail it. `DOVETAIL_WRITE_OSSIE_GOLDEN` rewrites it.
fn matches_golden(yaml: &str, golden: &str) {
    validate(yaml);
    assert_eq!(write(&read(yaml).unwrap()), yaml, "the model reads back");
    let generator = format!("dovetail {}\"", env!("CARGO_PKG_VERSION"));
    assert!(yaml.contains(&generator));
    let yaml = yaml.replace(&generator, "dovetail VERSION\"");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(golden);
    if std::env::var_os("DOVETAIL_WRITE_OSSIE_GOLDEN").is_some() {
        std::fs::write(&path, &yaml).unwrap();
    }
    let held = std::fs::read_to_string(&path).expect("the golden exists");
    assert_eq!(yaml, held, "{golden}");
    validate(&held);
}

#[test]
fn goldens_match() {
    let (sample, full) = passes();
    let s = read(&sample).unwrap();
    assert_eq!(s.meta.pass, Some(Verified::Sample));
    assert_eq!(s.meta.sample_rows, Some(SAMPLE_ROWS));
    let f = read(&full).unwrap();
    assert_eq!(f.meta.pass, Some(Verified::Full));
    assert_eq!(f.meta.sample_rows, Some(SAMPLE_ROWS));
    assert!(f
        .relationships
        .iter()
        .all(|r| r.meta.verified == Verified::Full));
    matches_golden(&sample, SAMPLE_GOLDEN);
    matches_golden(&full, FULL_GOLDEN);
}

// --- Brightfield's keys ------------------------------------------------------------

/// A JSON type, as Brightfield's types decode it.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Str,
    /// A whole number of at least zero: Rust's `u64`.
    Count,
    Number,
    Bool,
    List,
    Object,
}

/// A key Brightfield's reader defines: its name, type, whether every element must
/// carry it (no `serde(default)` there), and the values its enum allows.
type Key = (&'static str, Kind, bool, &'static [&'static str]);

const ROLES: &[&str] = &[
    "entity",
    "event",
    "reference",
    "bridge",
    "snapshot",
    "unclear",
];
const DECIDED_BY: &[&str] = &["dovetail", "analyst"];
const ANY: &[&str] = &[];

/// Brightfield's `ModelMeta`.
const MODEL: &[Key] = &[
    ("generator", Kind::Str, true, ANY),
    ("rejected", Kind::List, false, ANY),
    ("skipped_sources", Kind::List, false, ANY),
];

/// Brightfield's `DatasetMeta`. `excluded` has a default there, but Brightfield writes
/// it always, and so does dovetail.
const DATASET: &[Key] = &[
    ("role", Kind::Str, true, ROLES),
    ("role_evidence", Kind::List, false, ANY),
    ("folded_into", Kind::Str, false, ANY),
    ("excluded", Kind::Bool, true, ANY),
    ("decided_by", Kind::Str, true, DECIDED_BY),
    ("source_file", Kind::Str, true, ANY),
    ("source_part", Kind::Str, false, ANY),
    ("rows", Kind::Count, false, ANY),
];

/// Brightfield's `FieldMeta`.
const FIELD: &[Key] = &[
    ("finetype", Kind::Str, false, ANY),
    ("decided_by", Kind::Str, true, DECIDED_BY),
];

/// Brightfield's `RelationshipMeta`.
const RELATIONSHIP: &[Key] = &[
    ("grade", Kind::Str, true, &["likely", "possible"]),
    ("evidence", Kind::Object, true, ANY),
    ("cardinality", Kind::Str, true, ANY),
    (
        "verified",
        Kind::Str,
        true,
        &["unmeasured", "sample", "full"],
    ),
    ("decided_by", Kind::Str, true, DECIDED_BY),
    ("warning", Kind::Str, false, ANY),
];

/// Brightfield's `Evidence`, inside a relationship's `evidence`.
const EVIDENCE: &[Key] = &[
    ("value_overlap", Kind::Number, true, ANY),
    ("orphans", Kind::Count, true, ANY),
    ("child_rows", Kind::Count, true, ANY),
    ("child_type", Kind::Str, false, ANY),
    ("parent_type", Kind::Str, false, ANY),
    ("types_agree", Kind::Bool, true, ANY),
    ("name_similarity", Kind::Number, true, ANY),
    ("parent_duplicates", Kind::Count, false, ANY),
];

/// Optional keys the retail goldens never write, and why: no `--base` gives them
/// rejected joins or skipped sources, no package names a part, every proposed join's
/// parent key is unique, and nothing the full pass measures weakens a join.
const NEVER_WRITTEN: &[&str] = &[
    "rejected",
    "skipped_sources",
    "source_part",
    "parent_duplicates",
    "warning",
];

fn is(kind: Kind, v: &Json) -> bool {
    match kind {
        Kind::Str => v.is_string(),
        Kind::Count => v.is_u64(),
        Kind::Number => v.is_number(),
        Kind::Bool => v.is_boolean(),
        Kind::List => v.is_array(),
        Kind::Object => v.is_object(),
    }
}

/// Check one element's Meridian object against `keys`, recording each key it holds.
fn check(at: &str, data: &Json, keys: &[Key], seen: &mut Vec<&'static str>) {
    let object = data.as_object().unwrap_or_else(|| panic!("{at}: {data}"));
    for &(name, kind, required, allowed) in keys {
        match object.get(name) {
            None => assert!(!required, "{at} has no {name}: {data}"),
            Some(v) => {
                assert!(is(kind, v), "{at}.{name} is not {kind:?}: {v}");
                if let (false, Some(s)) = (allowed.is_empty(), v.as_str()) {
                    assert!(
                        allowed.contains(&s),
                        "{at}.{name} is {s}, not one of {allowed:?}"
                    );
                }
                seen.push(name);
            }
        }
    }
}

/// The parsed `data` of the element's one MERIDIAN extension.
fn meridian(at: &str, element: &Yaml) -> Json {
    let entries: Vec<&Yaml> = element["custom_extensions"]
        .as_sequence()
        .map(|s| {
            s.iter()
                .filter(|e| e["vendor_name"].as_str() == Some("MERIDIAN"))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        entries.len(),
        1,
        "{at} has {} MERIDIAN entries",
        entries.len()
    );
    let data = entries[0]["data"]
        .as_str()
        .unwrap_or_else(|| panic!("{at}: data is not a string"));
    serde_json::from_str(data).unwrap_or_else(|e| panic!("{at}: {e}: {data}"))
}

fn check_relationship(at: &str, data: &Json, seen: &mut Vec<&'static str>) {
    check(at, data, RELATIONSHIP, seen);
    check(&format!("{at}.evidence"), &data["evidence"], EVIDENCE, seen);
}

/// Every Meridian object in the model `yaml`, checked; the keys seen.
fn check_model(yaml: &str) -> Vec<&'static str> {
    let doc: Yaml = serde_yaml::from_str(yaml).unwrap();
    let mut seen = Vec::new();
    let model = meridian("model", &doc);
    check("model", &model, MODEL, &mut seen);
    for (i, r) in model["rejected"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let r: Yaml = serde_json::from_value(r.clone()).unwrap();
        let at = format!("rejected[{i}]");
        check_relationship(&at, &meridian(&at, &r), &mut seen);
    }
    for d in doc["datasets"].as_sequence().unwrap() {
        let name = d["name"].as_str().unwrap();
        check(name, &meridian(name, d), DATASET, &mut seen);
        for f in d["fields"].as_sequence().unwrap() {
            let at = format!("{name}.{}", f["name"].as_str().unwrap());
            check(&at, &meridian(&at, f), FIELD, &mut seen);
        }
    }
    for r in doc["relationships"].as_sequence().unwrap() {
        let at = r["name"].as_str().unwrap();
        check_relationship(at, &meridian(at, r), &mut seen);
    }
    seen
}

#[test]
fn goldens_carry_every_key_brightfield_reads() {
    let mut seen = Vec::new();
    for golden in [SAMPLE_GOLDEN, FULL_GOLDEN] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(golden);
        let yaml = std::fs::read_to_string(&path).expect("the golden exists");
        let keys = check_model(&yaml);
        assert!(!keys.is_empty(), "{golden}");
        seen.extend(keys);
    }
    for &(name, _, _, _) in [MODEL, DATASET, FIELD, RELATIONSHIP, EVIDENCE]
        .concat()
        .iter()
    {
        if NEVER_WRITTEN.contains(&name) {
            assert!(
                !seen.contains(&name),
                "{name} is written now: take it off NEVER_WRITTEN"
            );
        } else {
            assert!(seen.contains(&name), "no golden writes {name}");
        }
    }
}

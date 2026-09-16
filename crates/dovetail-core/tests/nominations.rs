//! `--nominations` support: a declared label wins outright over detection,
//! carries its taxonomy bounds into `constraints`, is scoped to its declared
//! stem, and a label the taxonomy does not carry is refused.

use std::path::{Path, PathBuf};

use dovetail_core::datapackage::assemble;
use dovetail_core::nominations::Nominations;
use dovetail_core::{Detector, SampledInput, ShapeHeuristicDetector};

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dovetail-nominations-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_csv(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

fn detect(path: &Path) -> dovetail_core::Detection {
    let input = SampledInput::from_path(path).unwrap();
    ShapeHeuristicDetector::new().detect(&input)
}

// AC1: a nominated column carries the declared label, the nomination marker,
// the taxonomy's type, and its validation bounds as `constraints` — while an
// undeclared column on the same resource is untouched.
#[test]
fn a_nominated_column_carries_the_label_marker_type_and_constraints() {
    let dir = tmp_dir("ac1");
    let csv = write_csv(
        &dir,
        "edgar.csv",
        "corpus,other\nhello world,1\nmore text,2\n",
    );
    let nominations = Nominations::parse(
        r#"{"resources":{"edgar":{"corpus":{"label":"representation.text.plain_text"}}}}"#,
        "n.json",
    )
    .unwrap();

    let det = detect(&csv);
    let dp = assemble(&det, &csv, "edgar", None, None, Some(&nominations)).unwrap();

    let corpus = &dp.resources[0]
        .schema
        .fields
        .iter()
        .find(|f| f.name == "corpus")
        .expect("corpus field");
    assert_eq!(corpus.ty, "string");
    assert_eq!(
        corpus.semantic_type.as_deref(),
        Some("representation.text.plain_text")
    );
    assert_eq!(corpus.nominated, Some(true));
    let constraints = corpus.constraints.as_ref().expect("constraints present");
    assert_eq!(
        constraints.get("minLength").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        constraints.get("maxLength").and_then(|v| v.as_u64()),
        Some(65536)
    );

    let other = dp.resources[0]
        .schema
        .fields
        .iter()
        .find(|f| f.name == "other")
        .expect("other field");
    assert_eq!(other.nominated, None, "undeclared column was nominated");
    assert!(other.constraints.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

// AC2: with no nominations at all, no field carries `constraints` or the
// nomination marker.
#[test]
fn no_nominations_carries_no_constraints_or_marker() {
    let dir = tmp_dir("ac2");
    let csv = write_csv(&dir, "edgar.csv", "corpus,other\nhello world,1\n");

    let det = detect(&csv);
    let dp = assemble(&det, &csv, "edgar", None, None, None).unwrap();
    let json = serde_json::to_value(&dp).unwrap();
    for field in json["resources"][0]["schema"]["fields"].as_array().unwrap() {
        assert!(field.get("constraints").is_none(), "{field}");
        assert!(field.get("x-finetype-nominated").is_none(), "{field}");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// AC2, sharper: a column detection typed on its own (no nomination at all)
// still carries no `constraints`. `ShapeHeuristicDetector` above never
// assigns a semantic type, so it cannot exercise the path where
// `col.semantic_type` is set without a nomination — this uses
// `FinetypeGuidedDetector`'s deterministic typing floor (no model dir
// needed) to put a real semantic type on an undeclared column and prove
// `constraints` is still `field_of`'s nomination-only property, not a
// semantic-type-implies-constraints one.
#[cfg(feature = "finetype-guided")]
#[test]
fn a_detected_but_unnominated_semantic_type_still_carries_no_constraints() {
    use dovetail_core::FinetypeGuidedDetector;

    let dir = tmp_dir("ac2-detected-type");
    let csv = write_csv(
        &dir,
        "signups.csv",
        "email\nada@example.com\ngrace@navy.mil\n",
    );

    let input = SampledInput::from_path(&csv).unwrap();
    let det = FinetypeGuidedDetector::from_env().detect(&input);
    let email = det
        .columns
        .iter()
        .find(|c| c.name == "email")
        .expect("email column");
    assert_eq!(
        email.semantic_type.as_deref(),
        Some("identity.person.email"),
        "detection did not resolve a semantic type — nothing for this test to guard"
    );

    let dp = assemble(&det, &csv, "signups", None, None, None).unwrap();
    let field = &dp.resources[0].schema.fields[0];
    assert!(field.constraints.is_none(), "{:?}", field.constraints);
    assert_eq!(field.nominated, None);

    let _ = std::fs::remove_dir_all(&dir);
}

// AC3: a nominated label whose taxonomy definition carries a `pattern`
// publishes the length bounds but no `pattern` and no `enum` — dovetail read
// no values under the nomination, so it makes no claim about them.
#[test]
fn a_pattern_bearing_nomination_publishes_bounds_but_no_pattern_or_enum() {
    let dir = tmp_dir("ac3");
    let csv = write_csv(&dir, "signups.csv", "email\nada@example.com\n");
    let nominations = Nominations::parse(
        r#"{"resources":{"signups":{"email":{"label":"identity.person.email"}}}}"#,
        "n.json",
    )
    .unwrap();

    let det = detect(&csv);
    let dp = assemble(&det, &csv, "signups", None, None, Some(&nominations)).unwrap();

    let field = &dp.resources[0].schema.fields[0];
    assert_eq!(field.ty, "string");
    assert_eq!(field.format.as_deref(), Some("email"));
    let constraints = field.constraints.as_ref().expect("constraints present");
    assert!(constraints.contains_key("minLength"), "{constraints:?}");
    assert!(constraints.contains_key("maxLength"), "{constraints:?}");
    assert!(!constraints.contains_key("pattern"), "{constraints:?}");
    assert!(!constraints.contains_key("enum"), "{constraints:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

// AC4: a nominations file declaring a column under one stem only nominates
// that resource's field and leaves an identically-named column on a
// differently-stemmed resource alone.
#[test]
fn a_nomination_applies_only_to_its_declared_stem() {
    let dir = tmp_dir("ac4");
    let csv_a = write_csv(&dir, "a.csv", "corpus\nhello\n");
    let csv_b = write_csv(&dir, "b.csv", "corpus\nworld\n");
    let nominations = Nominations::parse(
        r#"{"resources":{"a":{"corpus":{"label":"representation.text.plain_text"}}}}"#,
        "n.json",
    )
    .unwrap();

    let det_a = detect(&csv_a);
    let dp_a = assemble(&det_a, &csv_a, "a", None, None, Some(&nominations)).unwrap();
    assert_eq!(dp_a.resources[0].schema.fields[0].nominated, Some(true));

    let det_b = detect(&csv_b);
    let dp_b = assemble(&det_b, &csv_b, "b", None, None, Some(&nominations)).unwrap();
    assert_eq!(dp_b.resources[0].schema.fields[0].nominated, None);

    let _ = std::fs::remove_dir_all(&dir);
}

// AC5: a nominated label the taxonomy does not carry is refused, naming the
// column and the label.
#[test]
fn an_unknown_nominated_label_is_refused_naming_the_column_and_label() {
    let dir = tmp_dir("ac5");
    let csv = write_csv(&dir, "edgar.csv", "corpus\nhello\n");
    let nominations = Nominations::parse(
        r#"{"resources":{"edgar":{"corpus":{"label":"not.a.real.label"}}}}"#,
        "n.json",
    )
    .unwrap();

    let det = detect(&csv);
    let err = assemble(&det, &csv, "edgar", None, None, Some(&nominations))
        .expect_err("an unknown label must be refused");
    let msg = err.to_string();
    assert!(msg.contains("corpus"), "{msg}");
    assert!(msg.contains("not.a.real.label"), "{msg}");

    let _ = std::fs::remove_dir_all(&dir);
}

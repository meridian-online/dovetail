//! The analyst's decisions carried through a proposal, over the golden retail model.

use dovetail_core::ossie::{
    carry, read, write, DecidedBy, Model, Relationship, Role, SkippedSource, Verified,
};
use serde_json::{json, Map};

const GOLDEN: &str = include_str!("../../../tests/ossie-fixtures/retail.sample.ossie.yaml");

fn proposal() -> Model {
    read(GOLDEN).unwrap()
}

fn key(r: &Relationship) -> (String, Vec<String>, String, Vec<String>) {
    (
        r.from.clone(),
        r.from_columns.clone(),
        r.to.clone(),
        r.to_columns.clone(),
    )
}

fn note(text: &str) -> Map<String, serde_json::Value> {
    let mut m = Map::new();
    m.insert("note".to_string(), json!(text));
    m
}

/// The golden with the analyst's hand on it.
fn base() -> Model {
    let mut b = proposal();

    // An analyst dataset with a changed role and a MERIDIAN `note`.
    let stores = b.datasets.iter_mut().find(|d| d.name == "stores").unwrap();
    stores.meta.decided_by = DecidedBy::Analyst;
    stores.meta.role = Role::Reference;
    stores.meta.extra = note("a lookup, not an entity");

    // An analyst field inside a dovetail-decided dataset.
    let customers = b
        .datasets
        .iter_mut()
        .find(|d| d.name == "customers")
        .unwrap();
    assert_eq!(customers.meta.decided_by, DecidedBy::Dovetail);
    let email = customers
        .fields
        .iter_mut()
        .find(|f| f.name == "email")
        .unwrap();
    email.meta.decided_by = DecidedBy::Analyst;
    email.meta.finetype = Some("identity.person.email".to_string());
    email.meta.extra = note("personal data");

    // A dovetail dataset the analyst excluded.
    b.datasets
        .iter_mut()
        .find(|d| d.name == "promo_codes")
        .unwrap()
        .meta
        .excluded = true;

    // A proposed join the analyst rejected: it moves from `relationships` to `rejected`.
    let at = b
        .relationships
        .iter()
        .position(|r| r.name == "loyalty_visits_stores")
        .unwrap();
    let mut rejected = b.relationships.remove(at);
    rejected.meta.decided_by = DecidedBy::Analyst;
    b.meta.rejected = vec![rejected];

    // An analyst join the proposal does not have.
    let mut added = b.relationships[0].clone();
    added.name = "stores_suppliers".to_string();
    added.from = "stores".to_string();
    added.from_columns = vec!["supplier_id".to_string()];
    added.to = "suppliers".to_string();
    added.to_columns = vec!["supplier_id".to_string()];
    added.meta.decided_by = DecidedBy::Analyst;
    added.meta.verified = Verified::Unmeasured;
    added.meta.extra = note("added by hand");
    b.relationships.push(added);

    b.meta.skipped_sources = vec![
        SkippedSource {
            path: "scans/receipt.pdf".to_string(),
            reason: "not a loadable format".to_string(),
            extra: Map::new(),
        },
        SkippedSource {
            path: "export/huge.json".to_string(),
            reason: "oversized".to_string(),
            extra: note("kept by Brightfield"),
        },
    ];
    b
}

/// A model holding one element only, so its YAML is that element's.
fn alone_dataset(m: &Model, name: &str) -> String {
    let mut out = shell(m);
    out.datasets = vec![m.datasets.iter().find(|d| d.name == name).unwrap().clone()];
    write(&out)
}

fn alone_field(m: &Model, dataset: &str, field: &str) -> String {
    let mut d = m
        .datasets
        .iter()
        .find(|d| d.name == dataset)
        .unwrap()
        .clone();
    d.fields.retain(|f| f.name == field);
    assert_eq!(d.fields.len(), 1);
    let mut out = shell(m);
    out.datasets = vec![d];
    write(&out)
}

fn alone_relationship(m: &Model, name: &str) -> String {
    let mut out = shell(m);
    out.relationships = vec![m
        .relationships
        .iter()
        .find(|r| r.name == name)
        .unwrap()
        .clone()];
    write(&out)
}

fn shell(m: &Model) -> Model {
    let mut out = m.clone();
    out.datasets.clear();
    out.relationships.clear();
    out.meta.rejected.clear();
    out.meta.skipped_sources.clear();
    out
}

#[test]
fn analyst_elements_are_byte_identical() {
    let base = base();
    let carried = carry(proposal(), &base);

    assert_eq!(
        alone_dataset(&base, "stores"),
        alone_dataset(&carried, "stores")
    );
    assert_eq!(
        alone_field(&base, "customers", "email"),
        alone_field(&carried, "customers", "email")
    );
    assert_eq!(
        alone_relationship(&base, "stores_suppliers"),
        alone_relationship(&carried, "stores_suppliers")
    );
    // The analyst's dataset kept its place; the analyst's field kept its place in its dataset.
    let names = |m: &Model| {
        m.datasets
            .iter()
            .map(|d| d.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&carried), names(&proposal()));
    let fields = |m: &Model| {
        m.datasets
            .iter()
            .find(|d| d.name == "customers")
            .unwrap()
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(fields(&carried), fields(&proposal()));
    // The analyst's join is appended after the proposal's.
    assert_eq!(
        carried.relationships.last().unwrap().name,
        "stores_suppliers"
    );
}

#[test]
fn dovetail_elements_stay_the_proposals() {
    let carried = carry(proposal(), &base());
    let p = proposal();
    let customers = |m: &Model| {
        let mut d = m
            .datasets
            .iter()
            .find(|d| d.name == "customers")
            .unwrap()
            .clone();
        d.fields.retain(|f| f.name != "email");
        d
    };
    assert_eq!(customers(&carried), customers(&p));
}

#[test]
fn rejected_join_is_not_proposed() {
    let base = base();
    let rejected = key(&base.meta.rejected[0]);
    // The proposal still proposes it, under a name of its own.
    let mut p = proposal();
    p.relationships
        .iter_mut()
        .find(|r| key(r) == rejected)
        .unwrap()
        .name = "renamed_by_the_proposal".to_string();
    assert!(p.relationships.iter().any(|r| key(r) == rejected));

    let carried = carry(p, &base);
    assert!(!carried.relationships.iter().any(|r| key(r) == rejected));
    assert_eq!(carried.relationships.len(), proposal().relationships.len());
}

#[test]
fn an_analyst_join_that_was_rejected_is_kept() {
    let mut base = base();
    let mut both = base.meta.rejected[0].clone();
    both.name = "kept_by_the_analyst".to_string();
    base.relationships.push(both);
    let carried = carry(proposal(), &base);
    assert!(carried
        .relationships
        .iter()
        .any(|r| r.name == "kept_by_the_analyst"));
}

#[test]
fn skipped_sources_and_rejected_carry() {
    let base = base();
    let mut p = proposal();
    p.meta.skipped_sources = vec![SkippedSource {
        path: "other.json".to_string(),
        reason: "from the run".to_string(),
        extra: Map::new(),
    }];
    let carried = carry(p, &base);
    assert_eq!(carried.meta.skipped_sources, base.meta.skipped_sources);
    assert_eq!(carried.meta.rejected, base.meta.rejected);
    // Everything else in the model meta is the proposal's.
    assert_eq!(carried.meta.generator, proposal().meta.generator);
}

#[test]
fn excluded_survives() {
    let carried = carry(proposal(), &base());
    let promo = carried
        .datasets
        .iter()
        .find(|d| d.name == "promo_codes")
        .unwrap();
    assert!(promo.meta.excluded);
    // Only `excluded` moved: the rest of the dataset is the proposal's.
    let mut expected = proposal()
        .datasets
        .into_iter()
        .find(|d| d.name == "promo_codes")
        .unwrap();
    expected.meta.excluded = true;
    assert_eq!(promo, &expected);
}

#[test]
fn an_analyst_dataset_the_proposal_lacks_is_appended() {
    let mut base = base();
    let mut extra = base.datasets[0].clone();
    extra.name = "hand_made".to_string();
    extra.meta.decided_by = DecidedBy::Analyst;
    base.datasets.push(extra);
    let mut p = proposal();
    p.datasets.retain(|d| d.name != "stores");
    let carried = carry(p, &base);
    let tail: Vec<_> = carried
        .datasets
        .iter()
        .rev()
        .take(2)
        .map(|d| d.name.as_str())
        .collect();
    // Base order: `stores` comes before `hand_made`.
    assert_eq!(tail, ["hand_made", "stores"]);
}

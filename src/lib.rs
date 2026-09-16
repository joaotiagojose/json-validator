//! Validate property listing data and classify its recorded advertising permission.
//!
//! A confirmed result describes the supplied data, not verified legal permission.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceState {
    Confirmed,
    Rejected,
    Unknown,
}

impl EvidenceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "Confirmed",
            Self::Rejected => "Rejected",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// JSON Pointer to the invalid value; an empty pointer means the document root.
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingReport {
    pub index: usize,
    pub id: Option<String>,
    pub issues: Vec<Issue>,
    /// Invalid listings have no evidence classification.
    pub evidence: Option<EvidenceState>,
    pub reason: String,
}

impl ListingReport {
    pub fn passed(&self) -> bool {
        self.issues.is_empty() && self.evidence == Some(EvidenceState::Confirmed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentReport {
    pub issues: Vec<Issue>,
    pub listings: Vec<ListingReport>,
}

impl DocumentReport {
    pub fn passed(&self) -> bool {
        self.issues.is_empty()
            && !self.listings.is_empty()
            && self.listings.iter().all(ListingReport::passed)
    }
}

/// Validate every record, accumulating errors instead of stopping at the first one.
pub fn validate(value: &Value) -> DocumentReport {
    let mut report = DocumentReport {
        issues: Vec::new(),
        listings: Vec::new(),
    };

    let Some(listings) = value.as_array() else {
        report
            .issues
            .push(issue("", "Expected an array of listings."));
        return report;
    };

    if listings.is_empty() {
        report
            .issues
            .push(issue("", "Expected at least one listing."));
        return report;
    }

    report.listings = listings
        .iter()
        .enumerate()
        .map(|(index, value)| validate_listing(index, value))
        .collect();

    let mut indices_by_id: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, value) in listings.iter().enumerate() {
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            let id = id.trim();
            if !id.is_empty() {
                indices_by_id.entry(id).or_default().push(index);
            }
        }
    }

    for (id, indices) in indices_by_id {
        if indices.len() > 1 {
            for index in indices {
                let listing = &mut report.listings[index];
                listing.issues.push(issue(
                    &format!("/{index}/id"),
                    &format!("Duplicate listing ID: {id}."),
                ));
                listing.evidence = None;
                listing.reason = "Listing contains validation errors.".to_owned();
            }
        }
    }

    report
}

fn validate_listing(index: usize, value: &Value) -> ListingReport {
    let base = format!("/{index}");
    let mut report = ListingReport {
        index,
        id: None,
        issues: Vec::new(),
        evidence: None,
        reason: "Listing contains validation errors.".to_owned(),
    };

    let Some(listing) = value.as_object() else {
        report
            .issues
            .push(issue(&base, "Expected a listing object."));
        return report;
    };

    report.id = required_text(listing, "id", &base, &mut report.issues);
    match listing.get("property_type").and_then(Value::as_str) {
        Some("Apartment" | "House" | "Land") => {}
        _ => report.issues.push(issue(
            &format!("{base}/property_type"),
            "Expected one of: Apartment, House, Land.",
        )),
    }
    required_text(listing, "location", &base, &mut report.issues);
    match listing.get("asking_price").and_then(Value::as_u64) {
        Some(price) if price > 0 => {}
        _ => report.issues.push(issue(
            &format!("{base}/asking_price"),
            "Expected a positive integer representable as u64.",
        )),
    }

    let (evidence, reason) = validate_permission(
        listing.get("advertising_permission"),
        &format!("{base}/advertising_permission"),
        &mut report.issues,
    );
    reject_unknown_fields(
        listing,
        &[
            "id",
            "property_type",
            "location",
            "asking_price",
            "advertising_permission",
        ],
        &base,
        &mut report.issues,
    );

    if report.issues.is_empty() {
        report.evidence = Some(evidence);
        report.reason = reason.to_owned();
    }

    report
}

fn required_text(
    object: &Map<String, Value>,
    field: &str,
    base: &str,
    issues: &mut Vec<Issue>,
) -> Option<String> {
    match object.get(field).and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        _ => {
            issues.push(issue(
                &format!("{base}/{field}"),
                "Expected a nonblank string.",
            ));
            None
        }
    }
}

fn validate_permission(
    value: Option<&Value>,
    base: &str,
    issues: &mut Vec<Issue>,
) -> (EvidenceState, &'static str) {
    let permission = match value {
        None | Some(Value::Null) => {
            return (
                EvidenceState::Unknown,
                "No advertising permission supplied.",
            );
        }
        Some(Value::Object(permission)) => permission,
        Some(_) => {
            issues.push(issue(base, "Expected an object or null."));
            return (EvidenceState::Unknown, "Invalid advertising permission.");
        }
    };

    let granted = match permission.get("granted") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(granted)) => Some(*granted),
        Some(_) => {
            issues.push(issue(
                &format!("{base}/granted"),
                "Expected a boolean or null.",
            ));
            None
        }
    };
    let source = match permission.get("source") {
        None | Some(Value::Null) => None,
        Some(Value::String(source)) => Some(source.trim()),
        Some(_) => {
            issues.push(issue(
                &format!("{base}/source"),
                "Expected a string or null.",
            ));
            None
        }
    };
    reject_unknown_fields(permission, &["granted", "source"], base, issues);

    let Some(granted) = granted else {
        return (
            EvidenceState::Unknown,
            "No explicit permission decision supplied.",
        );
    };
    if source.is_none_or(str::is_empty) {
        return (
            EvidenceState::Unknown,
            "No nonblank permission source supplied.",
        );
    }

    match granted {
        true => (
            EvidenceState::Confirmed,
            "Permission is explicitly granted and includes a source.",
        ),
        false => (
            EvidenceState::Rejected,
            "Permission is explicitly refused and includes a source.",
        ),
    }
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    base: &str,
    issues: &mut Vec<Issue>,
) {
    let mut unknown: Vec<_> = object
        .keys()
        .filter(|field| !allowed.contains(&field.as_str()))
        .collect();
    unknown.sort();
    for field in unknown {
        let escaped = field.replace('~', "~0").replace('/', "~1");
        issues.push(issue(&format!("{base}/{escaped}"), "Unsupported field."));
    }
}

fn issue(path: &str, message: &str) -> Issue {
    Issue {
        path: path.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn confirmed_listing() -> Value {
        json!({
            "id": "DEMO-001",
            "property_type": "House",
            "location": "Tavira",
            "asking_price": 250_000,
            "advertising_permission": {"granted": true, "source": "fictional-permission"}
        })
    }

    fn report_for(listing: Value) -> ListingReport {
        validate(&Value::Array(vec![listing])).listings.remove(0)
    }

    #[test]
    fn sample_has_two_confirmed_one_rejected_and_five_unknown() {
        let fixture: Value = serde_json::from_str(include_str!("../data/listings.json")).unwrap();
        let report = validate(&fixture);
        let states: Vec<_> = report.listings.iter().map(|item| item.evidence).collect();
        assert_eq!(
            states,
            vec![
                Some(EvidenceState::Confirmed),
                Some(EvidenceState::Confirmed),
                Some(EvidenceState::Rejected),
                Some(EvidenceState::Unknown),
                Some(EvidenceState::Unknown),
                Some(EvidenceState::Unknown),
                Some(EvidenceState::Unknown),
                Some(EvidenceState::Unknown),
            ]
        );
        assert!(report.issues.is_empty());
        assert!(report.listings.iter().all(|item| item.issues.is_empty()));
        assert!(!report.passed());
    }

    #[test]
    fn every_invalid_fixture_record_reports_the_expected_field() {
        let fixture: Value =
            serde_json::from_str(include_str!("../data/invalid-listings.json")).unwrap();
        let report = validate(&fixture);
        let fields = [
            "asking_price",
            "asking_price",
            "property_type",
            "property_type",
            "location",
            "advertising_permission/granted",
            "advertising_permission/source",
            "asking_price",
        ];
        assert_eq!(report.listings.len(), fields.len());
        for (listing, field) in report.listings.iter().zip(fields) {
            assert_eq!(listing.issues.len(), 1);
            assert_eq!(
                listing.issues[0].path,
                format!("/{}/{field}", listing.index)
            );
            assert_eq!(listing.evidence, None);
            assert!(!listing.passed());
        }
        assert!(!report.passed());
    }

    #[test]
    fn only_a_nonempty_document_of_valid_confirmed_listings_passes() {
        let listing = confirmed_listing();
        assert!(report_for(listing.clone()).passed());
        assert!(validate(&json!([listing])).passed());
        for value in [
            json!([]),
            json!({}),
            json!(null),
            json!(false),
            json!(3),
            json!("text"),
        ] {
            let report = validate(&value);
            assert!(!report.passed());
            assert_eq!(report.issues.len(), 1);
            assert_eq!(report.issues[0].path, "");
            assert!(report.listings.is_empty());
        }
    }

    #[test]
    fn invalid_records_do_not_stop_later_records_being_validated() {
        let report = validate(&json!([null, [], 5, confirmed_listing()]));
        assert_eq!(report.listings.len(), 4);
        for (index, listing) in report.listings[..3].iter().enumerate() {
            assert_eq!(listing.issues[0].path, format!("/{index}"));
            assert_eq!(listing.evidence, None);
        }
        assert!(report.listings[3].passed());
        assert!(!report.passed());
    }

    #[test]
    fn duplicate_ids_invalidate_every_occurrence_after_trimming() {
        let a = confirmed_listing();
        let mut b = a.clone();
        b["id"] = json!(" DEMO-001 ");
        let mut c = a.clone();
        c["id"] = json!("DEMO-002");
        let report = validate(&json!([a, b, c]));
        for (index, listing) in report.listings[..2].iter().enumerate() {
            assert_eq!(listing.evidence, None);
            assert_eq!(listing.issues[0].path, format!("/{index}/id"));
            assert!(listing.issues[0].message.contains("Duplicate"));
        }
        assert!(report.listings[2].passed());
    }

    #[test]
    fn collects_all_required_field_errors_in_a_stable_order() {
        let report = report_for(json!({}));
        let paths: Vec<_> = report
            .issues
            .iter()
            .map(|issue| issue.path.as_str())
            .collect();
        assert_eq!(
            paths,
            [
                "/0/id",
                "/0/property_type",
                "/0/location",
                "/0/asking_price"
            ]
        );
        assert_eq!(report.evidence, None);
    }

    #[test]
    fn required_text_rejects_blank_and_nonstring_values() {
        for field in ["id", "location"] {
            for value in [
                json!(null),
                json!(" \t\n"),
                json!(12),
                json!(true),
                json!([]),
                json!({}),
            ] {
                let mut listing = confirmed_listing();
                listing[field] = value;
                let report = report_for(listing);
                assert_eq!(report.issues[0].path, format!("/0/{field}"));
                assert_eq!(report.evidence, None);
            }
        }
    }

    #[test]
    fn property_types_are_an_exact_closed_set() {
        for value in [json!("Apartment"), json!("House"), json!("Land")] {
            let mut listing = confirmed_listing();
            listing["property_type"] = value;
            assert!(report_for(listing).passed());
        }
        for value in [
            json!("house"),
            json!(" House "),
            json!("Castle"),
            json!(null),
            json!(1),
        ] {
            let mut listing = confirmed_listing();
            listing["property_type"] = value;
            assert_eq!(report_for(listing).issues[0].path, "/0/property_type");
        }
    }

    #[test]
    fn price_requires_a_positive_u64_integer() {
        for value in [json!(1), json!(u64::MAX)] {
            let mut listing = confirmed_listing();
            listing["asking_price"] = value;
            assert!(report_for(listing).passed());
        }
        for value in [
            json!(0),
            json!(-1),
            json!(1.0),
            json!(1.5),
            json!(1e30),
            json!("1"),
            json!(null),
            json!(true),
        ] {
            let mut listing = confirmed_listing();
            listing["asking_price"] = value;
            let report = report_for(listing);
            assert_eq!(report.issues[0].path, "/0/asking_price");
            assert_eq!(report.evidence, None);
        }
    }

    #[test]
    fn parsed_price_tokens_preserve_integer_limits_and_reject_float_forms() {
        let mut listing = confirmed_listing();
        listing["asking_price"] = serde_json::from_str("18446744073709551615").unwrap();
        assert!(report_for(listing.clone()).passed());

        for token in [
            "18446744073709551616",
            "18446744073709551615.0",
            "1.0",
            "1e0",
        ] {
            listing["asking_price"] = serde_json::from_str(token).unwrap();
            let report = report_for(listing.clone());
            assert_eq!(report.issues[0].path, "/0/asking_price", "token: {token}");
            assert_eq!(report.evidence, None);
        }
    }

    #[test]
    fn missing_evidence_never_confirms_or_rejects_permission() {
        for permission in [
            json!(null),
            json!({}),
            json!({"source": "ref"}),
            json!({"granted": null, "source": "ref"}),
            json!({"granted": true}),
            json!({"granted": false}),
            json!({"granted": true, "source": " "}),
            json!({"granted": false, "source": null}),
        ] {
            let mut listing = confirmed_listing();
            listing["advertising_permission"] = permission;
            let report = report_for(listing);
            assert!(report.issues.is_empty());
            assert_eq!(report.evidence, Some(EvidenceState::Unknown));
            assert!(!report.passed());
        }
        let mut listing = confirmed_listing();
        listing
            .as_object_mut()
            .unwrap()
            .remove("advertising_permission");
        assert_eq!(report_for(listing).evidence, Some(EvidenceState::Unknown));
    }

    #[test]
    fn explicit_refusal_with_a_source_is_rejected() {
        let mut listing = confirmed_listing();
        listing["advertising_permission"]["granted"] = json!(false);
        let report = report_for(listing);
        assert!(report.issues.is_empty());
        assert_eq!(report.evidence, Some(EvidenceState::Rejected));
        assert!(!report.passed());
    }

    #[test]
    fn permission_types_are_validated_without_coercion() {
        for permission in [json!(true), json!("yes"), json!([]), json!(1)] {
            let mut listing = confirmed_listing();
            listing["advertising_permission"] = permission;
            let report = report_for(listing);
            assert_eq!(report.issues[0].path, "/0/advertising_permission");
            assert_eq!(report.evidence, None);
        }
        let mut listing = confirmed_listing();
        listing["advertising_permission"] = json!({"granted": "true", "source": 12});
        let report = report_for(listing);
        assert_eq!(report.issues.len(), 2);
        assert_eq!(report.issues[0].path, "/0/advertising_permission/granted");
        assert_eq!(report.issues[1].path, "/0/advertising_permission/source");
        assert_eq!(report.evidence, None);
    }

    #[test]
    fn unsupported_evidence_metadata_cannot_produce_a_confirmed_result() {
        let mut listing = confirmed_listing();
        listing["advertising_permission"]["expired"] = json!(true);
        listing["conditional"] = json!(true);
        let report = report_for(listing);
        assert_eq!(report.issues.len(), 2);
        assert_eq!(report.issues[0].path, "/0/advertising_permission/expired");
        assert_eq!(report.issues[1].path, "/0/conditional");
        assert_eq!(report.evidence, None);
        assert!(!report.passed());
    }

    #[test]
    fn unknown_field_paths_escape_json_pointer_characters() {
        let mut listing = confirmed_listing();
        listing["extra~/field"] = json!(true);
        assert_eq!(report_for(listing).issues[0].path, "/0/extra~0~1field");
    }
}

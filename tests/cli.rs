use serde_json::{Value, json};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "json-validator-cli-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_json-validator"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(name)
}

fn valid_listing(id: &str) -> Value {
    json!({
        "id": id,
        "property_type": "Apartment",
        "location": "Tavira",
        "asking_price": 245000,
        "advertising_permission": {
            "granted": true,
            "source": "fictional-permission"
        }
    })
}

fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json_report(path: &Path, exit_code: i32) -> Value {
    let output = command()
        .arg(path)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert_exit(&output, exit_code);
    assert!(
        output.stderr.is_empty(),
        "JSON reports should contain file errors: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout must be one JSON document")
}

#[test]
fn confirmed_listing_passes_with_machine_readable_report() {
    let temp = TempDir::new();
    let path = temp.write(
        "valid.json",
        &json!([valid_listing("DEMO-001")]).to_string(),
    );
    let report = json_report(&path, 0);

    assert_eq!(report["passed"], true);
    assert_eq!(report["summary"]["files"], 1);
    assert_eq!(report["summary"]["listings"], 1);
    assert_eq!(report["summary"]["passed"], 1);
    assert_eq!(report["summary"]["confirmed"], 1);
    assert_eq!(report["summary"]["invalid"], 0);
    assert_eq!(report["summary"]["file_errors"], 0);

    let file = &report["files"][0];
    assert!(file["path"].as_str().unwrap().ends_with("valid.json"));
    assert!(file["error"].is_null());
    assert!(file["issues"].as_array().unwrap().is_empty());
    let listing = &file["listings"][0];
    assert_eq!(listing["index"], 0);
    assert_eq!(listing["id"], "DEMO-001");
    assert_eq!(listing["valid"], true);
    assert_eq!(listing["passed"], true);
    assert_eq!(listing["evidence"], "Confirmed");
    assert!(!listing["reason"].as_str().unwrap().is_empty());
    assert!(listing["issues"].as_array().unwrap().is_empty());
}

#[test]
fn mixed_fixture_preserves_rejected_and_unknown_evidence() {
    let report = json_report(&fixture("listings.json"), 1);

    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["listings"], 8);
    assert_eq!(report["summary"]["passed"], 2);
    assert_eq!(report["summary"]["invalid"], 0);
    assert_eq!(report["summary"]["confirmed"], 2);
    assert_eq!(report["summary"]["rejected"], 1);
    assert_eq!(report["summary"]["unknown"], 5);
    let listings = report["files"][0]["listings"].as_array().unwrap();
    assert_eq!(listings[2]["evidence"], "Rejected");
    assert_eq!(listings[6]["evidence"], "Unknown");
    assert_eq!(listings[7]["evidence"], "Unknown");
    assert!(listings.iter().all(|listing| listing["valid"] == true));
    assert!(
        listings[2..]
            .iter()
            .all(|listing| listing["passed"] == false)
    );
}

#[test]
fn invalid_fixture_reports_every_record_without_classifying_evidence() {
    let report = json_report(&fixture("invalid-listings.json"), 1);

    assert_eq!(report["summary"]["listings"], 8);
    assert_eq!(report["summary"]["invalid"], 8);
    assert_eq!(report["summary"]["passed"], 0);
    let listings = report["files"][0]["listings"].as_array().unwrap();
    assert_eq!(listings.len(), 8);
    for (index, listing) in listings.iter().enumerate() {
        assert_eq!(listing["index"], index);
        assert_eq!(listing["valid"], false);
        assert_eq!(listing["passed"], false);
        assert!(listing["evidence"].is_null());
        let issues = listing["issues"].as_array().unwrap();
        assert!(!issues.is_empty());
        for issue in issues {
            assert!(!issue["path"].as_str().unwrap().is_empty());
            assert!(!issue["message"].as_str().unwrap().is_empty());
        }
    }
}

#[test]
fn malformed_json_reports_parse_location() {
    let report = json_report(&fixture("malformed.json"), 1);

    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["file_errors"], 1);
    assert_eq!(report["summary"]["listings"], 0);
    let file = &report["files"][0];
    assert!(file["listings"].as_array().unwrap().is_empty());
    assert_eq!(file["error"]["kind"], "parse");
    assert!(!file["error"]["message"].as_str().unwrap().is_empty());
    assert!(file["error"]["line"].as_u64().unwrap() > 0);
    assert!(file["error"]["column"].as_u64().unwrap() > 0);
}

#[test]
fn duplicate_object_keys_cannot_override_a_refused_permission() {
    let temp = TempDir::new();
    let path = temp.write(
        "duplicate-keys.json",
        r#"[{
            "id": "DEMO-001",
            "property_type": "Apartment",
            "location": "Tavira",
            "asking_price": 245000,
            "advertising_permission": {
                "granted": false,
                "granted": true,
                "source": "fictional-permission"
            }
        }]"#,
    );
    let report = json_report(&path, 1);

    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["passed"], 0);
    assert_eq!(report["files"][0]["error"]["kind"], "parse");
}

#[test]
fn file_exceeding_size_limit_is_rejected_before_parsing() {
    let temp = TempDir::new();
    let path = temp.0.join("oversized.json");
    fs::write(&path, vec![b' '; 10 * 1024 * 1024 + 1]).unwrap();
    let report = json_report(&path, 1);

    assert_eq!(report["passed"], false);
    assert_eq!(report["files"][0]["error"]["kind"], "input");
    assert_eq!(report["summary"]["file_errors"], 1);
    assert_eq!(report["summary"]["listings"], 0);
}

#[test]
fn missing_input_has_an_operational_exit_code_and_json_error() {
    let temp = TempDir::new();
    let report = json_report(&temp.0.join("missing.json"), 2);

    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["file_errors"], 1);
    assert_eq!(report["files"][0]["error"]["kind"], "io");
}

#[test]
fn empty_directory_is_an_operational_error() {
    let temp = TempDir::new();
    let report = json_report(&temp.0, 2);

    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["listings"], 0);
    assert_eq!(report["summary"]["file_errors"], 1);
}

#[test]
fn directory_is_sorted_nonrecursive_and_continues_after_parse_failure() {
    let temp = TempDir::new();
    let valid = json!([valid_listing("DEMO-001")]).to_string();
    temp.write("z-last.JSON", &valid);
    temp.write("a-first.json", "[ malformed");
    temp.write("ignored.txt", "not JSON");
    let nested = temp.0.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("ignored.json"), "not JSON").unwrap();

    let report = json_report(&temp.0, 1);

    let files = report["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0]["path"].as_str().unwrap().ends_with("a-first.json"));
    assert!(files[1]["path"].as_str().unwrap().ends_with("z-last.JSON"));
    assert_eq!(files[0]["error"]["kind"], "parse");
    assert_eq!(files[1]["listings"][0]["passed"], true);
    assert_eq!(report["summary"]["files"], 2);
    assert_eq!(report["summary"]["listings"], 1);
    assert_eq!(report["summary"]["passed"], 1);
    assert_eq!(report["summary"]["file_errors"], 1);
    assert_eq!(report["passed"], false);
}

#[test]
fn json_in_subdirectory_does_not_make_parent_nonempty() {
    let temp = TempDir::new();
    let nested = temp.0.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(
        nested.join("valid.json"),
        json!([valid_listing("DEMO-001")]).to_string(),
    )
    .unwrap();

    let report = json_report(&temp.0, 2);

    assert_eq!(report["summary"]["listings"], 0);
    assert_eq!(report["passed"], false);
}

#[cfg(unix)]
#[test]
fn directory_scan_excludes_symbolic_links() {
    let temp = TempDir::new();
    let target = temp.write(
        "target.txt",
        &json!([valid_listing("DEMO-001")]).to_string(),
    );
    std::os::unix::fs::symlink(target, temp.0.join("linked.json")).unwrap();
    let report = json_report(&temp.0, 2);

    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["listings"], 0);
}

#[test]
fn invalid_document_roots_and_empty_arrays_fail_validation() {
    let temp = TempDir::new();
    for (name, contents) in [
        ("object.json", "{}"),
        ("scalar.json", "42"),
        ("empty.json", "[]"),
    ] {
        let path = temp.write(name, contents);
        let report = json_report(&path, 1);
        assert_eq!(report["passed"], false);
        assert_eq!(report["summary"]["listings"], 0);
        let file = &report["files"][0];
        assert!(!file["issues"].as_array().unwrap().is_empty());
    }
}

#[test]
fn nonobject_records_fail_without_hiding_other_records() {
    let temp = TempDir::new();
    let path = temp.write(
        "records.json",
        &json!([null, valid_listing("DEMO-001")]).to_string(),
    );
    let report = json_report(&path, 1);

    assert_eq!(report["summary"]["listings"], 2);
    assert_eq!(report["summary"]["invalid"], 1);
    assert_eq!(report["summary"]["passed"], 1);
    let records = report["files"][0]["listings"].as_array().unwrap();
    assert_eq!(records[0]["index"], 0);
    assert!(records[0]["id"].is_null());
    assert_eq!(records[0]["valid"], false);
    assert!(records[0]["evidence"].is_null());
    assert_eq!(records[1]["index"], 1);
    assert_eq!(records[1]["passed"], true);
}

#[test]
fn duplicate_ids_cannot_produce_a_passing_report() {
    let temp = TempDir::new();
    let record = valid_listing("DUPLICATE");
    let path = temp.write("duplicates.json", &json!([record, record]).to_string());
    let report = json_report(&path, 1);

    assert_eq!(report["passed"], false);
    assert!(report["summary"]["invalid"].as_u64().unwrap() > 0);
    let records = report["files"][0]["listings"].as_array().unwrap();
    assert!(records.iter().any(|record| {
        record["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["path"].as_str().unwrap().ends_with("id"))
    }));
}

#[test]
fn help_and_version_succeed_without_an_input() {
    let help = command().arg("--help").output().unwrap();
    assert_exit(&help, 0);
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("--format"));
    assert!(text.contains("json-validator"));

    let version = command().arg("--version").output().unwrap();
    assert_exit(&version, 0);
    let text = String::from_utf8(version.stdout).unwrap();
    assert!(text.contains("json-validator"));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn bad_arguments_are_usage_errors_on_stderr() {
    let cases: &[&[&str]] = &[
        &[],
        &["--unknown"],
        &["--format"],
        &["data/listings.json", "--format", "yaml"],
        &["data/listings.json", "data/invalid-listings.json"],
    ];
    for args in cases {
        let output = command().args(*args).output().unwrap();
        assert_exit(&output, 2);
        assert!(!output.stderr.is_empty(), "arguments: {args:?}");
    }
}

#[test]
fn format_option_can_precede_input_and_text_is_the_default() {
    let temp = TempDir::new();
    let path = temp.write(
        "valid.json",
        &json!([valid_listing("DEMO-001")]).to_string(),
    );
    let json_output = command()
        .args([OsStr::new("--format"), OsStr::new("json"), path.as_os_str()])
        .output()
        .unwrap();
    assert_exit(&json_output, 0);
    let report: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(report["passed"], true);

    let default_output = command().arg(&path).output().unwrap();
    let explicit_output = command()
        .arg(&path)
        .args(["--format", "text"])
        .output()
        .unwrap();
    assert_exit(&default_output, 0);
    assert_exit(&explicit_output, 0);
    assert!(!default_output.stdout.is_empty());
    assert_eq!(default_output.stdout, explicit_output.stdout);
    assert!(
        String::from_utf8(default_output.stdout)
            .unwrap()
            .contains("DEMO-001")
    );
}

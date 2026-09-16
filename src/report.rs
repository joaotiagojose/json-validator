use std::io::{self, Write};
use std::path::PathBuf;

use json_validator::{DocumentReport, EvidenceState, Issue};
use serde_json::{Value, json};

pub struct InputError {
    pub kind: &'static str,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl InputError {
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: "io",
            message: error.to_string(),
            line: None,
            column: None,
        }
    }

    pub fn input(message: &str) -> Self {
        Self {
            kind: "input",
            message: message.into(),
            line: None,
            column: None,
        }
    }

    pub fn parse(error: serde_json::Error) -> Self {
        Self {
            kind: "parse",
            message: error.to_string(),
            line: Some(error.line()),
            column: Some(error.column()),
        }
    }
}

pub struct FileReport {
    pub path: PathBuf,
    pub result: Result<DocumentReport, InputError>,
}

pub fn exit_code(files: &[FileReport]) -> u8 {
    if files
        .iter()
        .any(|file| matches!(&file.result, Err(error) if error.kind == "io"))
    {
        2
    } else if !files.is_empty()
        && files
            .iter()
            .all(|file| file.result.as_ref().is_ok_and(DocumentReport::passed))
    {
        0
    } else {
        1
    }
}

#[derive(Default)]
struct Summary {
    listings: usize,
    passed: usize,
    invalid: usize,
    confirmed: usize,
    rejected: usize,
    unknown: usize,
    file_errors: usize,
}

fn summarize(files: &[FileReport]) -> Summary {
    let mut summary = Summary::default();
    for file in files {
        match &file.result {
            Err(_) => summary.file_errors += 1,
            Ok(document) => {
                if !document.issues.is_empty() {
                    summary.file_errors += 1;
                }
                for listing in &document.listings {
                    summary.listings += 1;
                    summary.passed += usize::from(listing.passed());
                    summary.invalid += usize::from(!listing.issues.is_empty());
                    match listing.evidence {
                        Some(EvidenceState::Confirmed) => summary.confirmed += 1,
                        Some(EvidenceState::Rejected) => summary.rejected += 1,
                        Some(EvidenceState::Unknown) => summary.unknown += 1,
                        None => (),
                    }
                }
            }
        }
    }
    summary
}

fn issues_json(issues: &[Issue]) -> Vec<Value> {
    issues
        .iter()
        .map(|issue| json!({"path": issue.path, "message": issue.message}))
        .collect()
}

pub fn write_json(output: &mut impl Write, files: &[FileReport]) -> io::Result<()> {
    let summary = summarize(files);
    let results: Vec<Value> = files.iter().map(|file| {
        match &file.result {
            Err(error) => json!({
                "path": file.path.to_string_lossy(),
                "error": { "kind": error.kind, "message": error.message, "line": error.line, "column": error.column },
                "issues": [], "listings": [],
            }),
            Ok(document) => {
                let listings: Vec<Value> = document.listings.iter().map(|listing| json!({
                    "index": listing.index,
                    "id": listing.id,
                    "valid": listing.issues.is_empty(),
                    "passed": listing.passed(),
                    "evidence": listing.evidence.map(EvidenceState::as_str),
                    "reason": listing.reason,
                    "issues": issues_json(&listing.issues),
                })).collect();
                json!({"path": file.path.to_string_lossy(), "error": null,
                    "issues": issues_json(&document.issues), "listings": listings})
            }
        }
    }).collect();
    let value = json!({
        "files": results,
        "summary": {
            "files": files.len(), "listings": summary.listings, "passed": summary.passed,
            "invalid": summary.invalid, "confirmed": summary.confirmed, "rejected": summary.rejected,
            "unknown": summary.unknown, "file_errors": summary.file_errors,
        },
        "passed": exit_code(files) == 0,
    });
    serde_json::to_writer_pretty(&mut *output, &value)?;
    writeln!(output)
}

pub fn terminal_text(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

fn write_issues(output: &mut impl Write, issues: &[Issue]) -> io::Result<()> {
    for issue in issues {
        writeln!(
            output,
            "    {}: {}",
            terminal_text(&issue.path),
            terminal_text(&issue.message)
        )?;
    }
    Ok(())
}

pub fn write_text(output: &mut impl Write, files: &[FileReport]) -> io::Result<()> {
    for file in files {
        writeln!(output, "{}", terminal_text(&file.path.to_string_lossy()))?;
        match &file.result {
            Err(error) => writeln!(
                output,
                "  ERROR ({}): {}",
                error.kind,
                terminal_text(&error.message)
            )?,
            Ok(document) => {
                write_issues(output, &document.issues)?;
                for listing in &document.listings {
                    let id = listing
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("listing #{}", listing.index + 1));
                    let status = listing.evidence.map_or("Invalid", EvidenceState::as_str);
                    let outcome = if listing.passed() { "PASS" } else { "FAIL" };
                    writeln!(output, "  {outcome} {} [{status}]", terminal_text(&id))?;
                    writeln!(output, "    {}", terminal_text(&listing.reason))?;
                    write_issues(output, &listing.issues)?;
                }
            }
        }
        writeln!(output)?;
    }
    let summary = summarize(files);
    writeln!(
        output,
        "Summary: {} file(s), {} listing(s), {} passed, {} invalid",
        files.len(),
        summary.listings,
        summary.passed,
        summary.invalid
    )?;
    writeln!(
        output,
        "Evidence: {} Confirmed, {} Rejected, {} Unknown; {} file error(s)",
        summary.confirmed, summary.rejected, summary.unknown, summary.file_errors
    )?;
    writeln!(
        output,
        "Result: {}",
        if exit_code(files) == 0 {
            "PASS"
        } else {
            "FAIL"
        }
    )
}

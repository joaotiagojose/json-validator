# json-validator

[![CI](https://github.com/joaotiagojose/json-validator/actions/workflows/ci.yml/badge.svg)](https://github.com/joaotiagojose/json-validator/actions/workflows/ci.yml)

A Rust command-line tool for checking property-listing JSON before an import or publication pipeline. Get actionable diagnostics in the terminal or a JSON report for automation.

**Unknown information never counts as confirmed.** The validator distinguishes invalid input from evidence that is confirmed, rejected, or insufficient.

- Validate a file or every `.json` file directly inside a folder, in a stable order.
- Report malformed JSON, missing fields, incorrect types, and invalid values.
- Keep validation issues and evidence outcomes visible for each listing.
- Use exit codes and machine-readable reports in scripts and CI.

## Quick start

Install [Rust](https://www.rust-lang.org/tools/install), then run from the repository root:

```sh
cargo run -- data/valid-listings.json
```

```text
data/valid-listings.json
  PASS DEMO-VALID-001 [Confirmed]
    Permission is explicitly granted and includes a source.
  PASS DEMO-VALID-002 [Confirmed]
    Permission is explicitly granted and includes a source.

Summary: 1 file(s), 2 listing(s), 2 passed, 0 invalid
Evidence: 2 Confirmed, 0 Rejected, 0 Unknown; 0 file error(s)
Result: PASS
```

Try a mixed batch to see how incomplete and rejected evidence is reported:

```sh
cargo run -- data/listings.json
```

All sample listings are fictional. The mixed examples deliberately produce a nonzero exit status.

## Usage

```sh
# Process the JSON files directly inside a folder
cargo run -- data

# Produce a JSON report suitable for scripts and CI
cargo run -- data --format json

# Show command-line options
cargo run -- --help
```

Each file is limited to 10 MiB. Duplicate object keys and unsupported fields are rejected; symlinks encountered while scanning a directory are skipped.

To install the executable locally:

```sh
cargo install --path . --locked
json-validator data/valid-listings.json
```

| Exit code | Meaning |
| --- | --- |
| `0` | Every listing is valid and its evidence outcome is `Confirmed`. |
| `1` | Input contains malformed JSON, validation failures, or an evidence outcome of `Rejected` or `Unknown`. |
| `2` | Command usage, file access, or input discovery failed. |

The validator evaluates assertions supplied in the input. It does not verify documents or property claims with external sources, or check whether evidence has expired. A `Confirmed` result is limited to the checks implemented here.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

GitHub Actions runs these checks on Windows and Linux and builds release executables as workflow artifacts.

# json-validator

A command-line validator for property-listing data stored in JSON, written in Rust.

## Current status

Under development. The repository contains the initial project structure and sample input data. The validation pipeline is not yet implemented.

## Goal

Read a local JSON file containing property listings, apply explicit validation rules, and produce a report explaining each result.

Core principle: **unknown information must never be treated as confirmed information**.

## Scope of the first version

- Support one documented JSON format and one local input file per run.
- Parse listing fields, including an identifier, property type, location, and asking price.
- Report malformed JSON, missing required fields, incorrect types, and invalid values.
- Evaluate evidence-dependent rules using explicit states: `Confirmed`, `Rejected`, and `Unknown`.
- Print a terminal report identifying the listing, the check, its outcome, and the reason.
- Add focused tests for valid input, invalid input, and incomplete evidence.

Sample input data is available in [data](data/). All sample listings are fictional.

## Evidence handling

Input errors and evidence outcomes are separate concepts. A malformed price is a validation error; an absent piece of evidence leaves the corresponding evidence check unknown.

| State | Meaning |
| --- | --- |
| `Confirmed` | Explicit evidence in the input satisfies the defined requirement. |
| `Rejected` | Explicit evidence in the input shows that the requirement is not met. |
| `Unknown` | Evidence is missing, incomplete, expired, contradictory, or otherwise insufficient. |

Only `Confirmed` passes an evidence-dependent check. Confirmation is limited to the rules and evidence supplied in the input; independent verification of property claims is outside this project's scope.

## Implementation roadmap

1. Implement JSON parsing and field validation.
2. Evaluate evidence states and generate diagnostic messages.
3. Produce terminal reports for multiple listings.
4. Add automated coverage for parsing, field rules, and evidence states.

## Outside the initial scope

- Web interfaces, HTTP servers, and asynchronous runtimes.
- Databases, authentication, and deployment.
- Scraping, external APIs, and live property verification.

Additional command-line options and report formats are planned after the core validation pipeline.

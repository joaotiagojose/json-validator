mod parse;
mod report;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use report::{FileReport, InputError};

const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const HELP: &str = "json-validator — validate property-listing JSON

Usage: json-validator <file-or-directory> [--format text|json]

Arguments:
  <file-or-directory>  JSON file or folder of .json files (non-recursive)

Options:
  --format text|json  Report format (default: text)
  -h, --help          Show this help
  -V, --version       Show version
  --                 Treat remaining arguments as paths

Only structurally valid listings with Confirmed evidence pass.
Directories are sorted; symlinks are skipped. Maximum file size: 10 MiB.
No external sources are contacted or independently verified.

Exit codes: 0 = all pass; 1 = invalid input or unconfirmed evidence;
            2 = usage, file access, or directory discovery error.
";

#[derive(Clone, Copy)]
enum Format {
    Text,
    Json,
}

enum Command {
    Help,
    Version,
    Validate { path: PathBuf, format: Format },
}

fn arguments() -> Result<Command, String> {
    let mut args = env::args_os().skip(1);
    let mut path = None;
    let mut format = Format::Text;
    let mut format_seen = false;
    let mut options = true;
    while let Some(arg) = args.next() {
        match arg.to_str().filter(|_| options) {
            Some("--help" | "-h") => return Ok(Command::Help),
            Some("--version" | "-V") => return Ok(Command::Version),
            Some("--") => options = false,
            Some("--format") => {
                if format_seen {
                    return Err("--format may only be specified once".into());
                }
                format_seen = true;
                format = match args.next().as_deref().and_then(|v| v.to_str()) {
                    Some("text") => Format::Text,
                    Some("json") => Format::Json,
                    _ => return Err("--format requires 'text' or 'json'".into()),
                };
            }
            Some(option) if option.starts_with('-') => {
                return Err(format!("unknown option: {option}"));
            }
            _ => set_path(&mut path, arg)?,
        }
    }
    let path = path.ok_or("provide a JSON file or directory; use --help for usage")?;
    Ok(Command::Validate { path, format })
}

fn set_path(path: &mut Option<PathBuf>, arg: OsString) -> Result<(), String> {
    if path.is_some() {
        return Err("provide exactly one file or directory".into());
    }
    *path = Some(PathBuf::from(arg));
    Ok(())
}

fn discover(path: &Path) -> io::Result<Vec<PathBuf>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "input must be a regular file or directory (symlinks are not followed)",
        ));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if entry.file_type()?.is_file()
            && child
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            files.push(child);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "directory contains no regular .json files",
        ));
    }
    Ok(files)
}

fn validate_file(path: PathBuf) -> FileReport {
    let result = (|| {
        let file = File::open(&path).map_err(InputError::io)?;
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(InputError::io)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(InputError::input("file exceeds the 10 MiB size limit"));
        }
        let value = parse::from_slice(&bytes).map_err(InputError::parse)?;
        Ok(json_validator::validate(&value))
    })();
    FileReport { path, result }
}

fn run(command: Command) -> io::Result<u8> {
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    let exit_code = match command {
        Command::Help => {
            write!(output, "{HELP}")?;
            0
        }
        Command::Version => {
            writeln!(output, "json-validator {}", env!("CARGO_PKG_VERSION"))?;
            0
        }
        Command::Validate { path, format } => {
            let files = match discover(&path) {
                Ok(paths) => paths.into_iter().map(validate_file).collect(),
                Err(error) => vec![FileReport {
                    path,
                    result: Err(InputError::io(error)),
                }],
            };
            match format {
                Format::Text => report::write_text(&mut output, &files)?,
                Format::Json => report::write_json(&mut output, &files)?,
            }
            report::exit_code(&files)
        }
    };
    output.flush()?;
    Ok(exit_code)
}

fn main() -> ExitCode {
    let command = match arguments() {
        Ok(command) => command,
        Err(error) => {
            eprintln!("error: {}", report::terminal_text(&error));
            return ExitCode::from(2);
        }
    };
    match run(command) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error writing report: {error}");
            ExitCode::from(2)
        }
    }
}

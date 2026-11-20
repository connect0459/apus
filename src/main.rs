use std::path::{Path, PathBuf};
use std::process::ExitCode;

use apus::driver::build;

const USAGE: &str = "usage: apus build <source.swift> [-o <output>]";

struct BuildArgs {
    source: PathBuf,
    output: PathBuf,
}

fn parse_args(args: &[String]) -> Option<BuildArgs> {
    let [command, rest @ ..] = args else {
        return None;
    };
    if command != "build" {
        return None;
    }
    let (source, rest) = rest.split_first()?;
    let output = match rest {
        [] => default_output(Path::new(source)),
        [flag, output] if flag == "-o" => PathBuf::from(output),
        _ => return None,
    };
    Some(BuildArgs {
        source: PathBuf::from(source),
        output,
    })
}

fn default_output(source: &Path) -> PathBuf {
    PathBuf::from(source.file_stem().unwrap_or(source.as_os_str()))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(parsed) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let source = match std::fs::read_to_string(&parsed.source) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("{}: error: {error}", parsed.source.display());
            return ExitCode::from(1);
        }
    };
    match build(&source, &parsed.output) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            match error.line() {
                Some(line) => {
                    eprintln!(
                        "{}:{line}: error: {}",
                        parsed.source.display(),
                        error.message()
                    )
                }
                None => eprintln!("{}: error: {}", parsed.source.display(), error.message()),
            }
            ExitCode::from(1)
        }
    }
}

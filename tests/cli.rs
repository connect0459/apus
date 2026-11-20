use std::path::Path;
use std::process::{Command, Output};

fn apus(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_apus"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn building_a_source_file_produces_a_runnable_program() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("hello.swift"),
        "func main() { print(\"hi\") }\n",
    )
    .unwrap();
    let built = apus(&["build", "hello.swift", "-o", "hello"], dir.path());
    assert!(built.status.success(), "{}", text(&built.stderr));
    let run = Command::new(dir.path().join("hello")).output().unwrap();
    assert_eq!(text(&run.stdout), "hi\n");
}

#[test]
fn without_an_output_flag_the_program_is_named_after_the_source_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("greet.swift"), "func main() { print(1) }\n").unwrap();
    let built = apus(&["build", "greet.swift"], dir.path());
    assert!(built.status.success(), "{}", text(&built.stderr));
    assert!(dir.path().join("greet").exists());
}

#[test]
fn a_type_error_is_reported_with_the_file_and_line_and_fails_the_build() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("bad.swift"),
        "func main() {\n  let a: Int = true\n}\n",
    )
    .unwrap();
    let built = apus(&["build", "bad.swift", "-o", "bad"], dir.path());
    assert_eq!(built.status.code(), Some(1));
    assert!(
        text(&built.stderr).starts_with("bad.swift:2: error:"),
        "{}",
        text(&built.stderr)
    );
    assert!(!dir.path().join("bad").exists());
}

#[test]
fn a_missing_source_file_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let built = apus(&["build", "nope.swift"], dir.path());
    assert_eq!(built.status.code(), Some(1));
    assert!(
        text(&built.stderr).contains("nope.swift"),
        "{}",
        text(&built.stderr)
    );
}

#[test]
fn invalid_usage_prints_the_usage_and_exits_with_status_two() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec![],
        vec!["build"],
        vec!["run", "x.swift"],
        vec!["build", "a.swift", "-o"],
    ] {
        let result = apus(&args, dir.path());
        assert_eq!(result.status.code(), Some(2), "{args:?}");
        assert!(text(&result.stderr).contains("usage:"), "{args:?}");
    }
}

use apus::cache::Cache;
use apus::driver::{BuildOptions, BuildReport, Linker, build_with};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn output(&self) -> PathBuf {
        self.dir.path().join("prog")
    }

    fn cache_dir(&self) -> PathBuf {
        self.dir.path().join(".apus-cache")
    }

    fn build(&self, source: &str) -> BuildReport {
        build_with(source, &self.output(), &BuildOptions::default()).unwrap()
    }

    fn build_with(&self, source: &str, options: &BuildOptions) -> BuildReport {
        build_with(source, &self.output(), options).unwrap()
    }

    fn run(&self) -> String {
        let output = Command::new(self.output()).output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }
}

fn program(values: &[i64]) -> String {
    let mut source = String::new();
    for (index, value) in values.iter().enumerate() {
        source.push_str(&format!("func f{index}() -> Int {{ return {value} }}\n"));
    }
    let sum: Vec<String> = (0..values.len()).map(|i| format!("f{i}()")).collect();
    source.push_str(&format!("func main() {{ print({}) }}\n", sum.join(" + ")));
    source
}

const FUNCTIONS: usize = 6;

fn values() -> Vec<i64> {
    (1..=FUNCTIONS as i64).collect()
}

#[test]
fn the_first_build_compiles_every_function_and_links() {
    let project = Project::new();
    let report = project.build(&program(&values()));
    assert!(!report.was_skipped());
    assert_eq!(report.compiled(), FUNCTIONS + 1);
    assert_eq!(report.reused(), 0);
    assert!(report.linked());
    assert_eq!(project.run(), "21\n");
}

#[test]
fn rebuilding_unchanged_source_does_nothing() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    let report = project.build(&source);
    assert!(report.was_skipped());
    assert_eq!(report.compiled(), 0);
    assert!(!report.linked());
    assert_eq!(project.run(), "21\n");
}

#[test]
fn editing_only_a_comment_reuses_everything_and_skips_linking() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    let report = project.build(&format!("// a note\n{source}"));
    assert!(!report.was_skipped());
    assert_eq!(report.compiled(), 0);
    assert_eq!(report.reused(), FUNCTIONS + 1);
    assert!(!report.linked());
}

#[test]
fn editing_one_function_recompiles_only_that_function() {
    let project = Project::new();
    project.build(&program(&values()));
    let mut edited = values();
    edited[2] = 100;
    let report = project.build(&program(&edited));
    assert_eq!(report.compiled(), 1);
    assert_eq!(report.reused(), FUNCTIONS);
    assert!(report.linked());
    assert_eq!(project.run(), "118\n");
}

#[test]
fn adding_an_unrelated_function_compiles_only_that_function() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    let report = project.build(&format!("{source}func extra() {{ }}\n"));
    assert_eq!(report.compiled(), 1);
    assert_eq!(report.reused(), FUNCTIONS + 1);
}

#[test]
fn changing_a_callees_result_type_recompiles_the_callee_and_its_caller() {
    let project = Project::new();
    let before = "func g() -> Int { return 1 }\nfunc h() { print(g()) }\nfunc other() { }\nfunc main() { h() }\n";
    let after = "func g() -> Bool { return true }\nfunc h() { print(g()) }\nfunc other() { }\nfunc main() { h() }\n";
    project.build(before);
    let report = project.build(after);
    assert_eq!(report.compiled(), 2);
    assert_eq!(report.reused(), 2);
    assert_eq!(project.run(), "true\n");
}

#[test]
fn changing_a_struct_recompiles_only_the_functions_using_it() {
    let project = Project::new();
    let before = "struct A { var x: Int }\nstruct B { var y: Int }\n\
        func useA() -> Int { let a = A(x: 1)\nreturn a.x }\n\
        func useB() -> Int { let b = B(y: 2)\nreturn b.y }\n\
        func main() { print(useA() + useB()) }\n";
    let after = before
        .replace(
            "struct A { var x: Int }",
            "struct A { var z: Bool; var x: Int }",
        )
        .replace("A(x: 1)", "A(z: true, x: 1)");
    project.build(before);
    let report = project.build(&after);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "3\n");
}

#[test]
fn a_deleted_program_is_relinked_without_recompiling() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    std::fs::remove_file(project.output()).unwrap();
    let report = project.build(&source);
    assert_eq!(report.compiled(), 0);
    assert_eq!(report.reused(), FUNCTIONS + 1);
    assert!(report.linked());
    assert_eq!(project.run(), "21\n");
}

#[test]
fn a_different_compiler_stamp_invalidates_the_cache() {
    let project = Project::new();
    let source = program(&values());
    project.build_with(&source, &BuildOptions::default().with_compiler_stamp(1));
    let report = project.build_with(&source, &BuildOptions::default().with_compiler_stamp(2));
    assert_eq!(report.compiled(), FUNCTIONS + 1);
    assert!(report.linked());
}

#[test]
fn a_corrupt_cache_is_rebuilt_instead_of_failing() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    std::fs::write(project.cache_dir().join("functions.pack"), b"corrupt").unwrap();
    std::fs::write(project.cache_dir().join("stamp"), b"corrupt").unwrap();
    let report = project.build(&source);
    assert_eq!(report.compiled(), FUNCTIONS + 1);
    assert_eq!(project.run(), "21\n");
}

#[test]
fn the_cache_keeps_only_the_functions_of_the_latest_build() {
    let project = Project::new();
    let mut current = values();
    project.build(&program(&current));
    for round in 0..4 {
        current[round] += 50;
        project.build(&program(&current));
    }
    assert_eq!(Cache::open(&project.cache_dir()).len(), FUNCTIONS + 1);
}

#[test]
fn a_failed_build_leaves_the_previous_program_and_cache_usable() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    let broken = source.replace("return 1 }", "return true }");
    assert!(build_with(&broken, &project.output(), &BuildOptions::default()).is_err());
    assert_eq!(project.run(), "21\n");
    let report = project.build(&source);
    assert!(report.was_skipped());
}

#[test]
fn the_cache_directory_can_be_placed_elsewhere() {
    let project = Project::new();
    let elsewhere = project.dir.path().join("elsewhere");
    let options = BuildOptions::default().with_cache_dir(elsewhere.clone());
    project.build_with(&program(&values()), &options);
    assert!(elsewhere.join("functions.pack").exists());
    assert!(!project.cache_dir().exists());
}

fn run_at(path: &Path) -> String {
    String::from_utf8(Command::new(path).output().unwrap().stdout).unwrap()
}

#[test]
fn both_linkers_produce_a_working_program() {
    for linker in [Linker::Ld, Linker::Cc] {
        let project = Project::new();
        project.build_with(
            &program(&values()),
            &BuildOptions::default().with_linker(linker),
        );
        assert_eq!(run_at(&project.output()), "21\n", "{linker:?}");
    }
}

#[test]
fn a_stale_sdk_location_is_refreshed() {
    let project = Project::new();
    std::fs::create_dir_all(project.cache_dir()).unwrap();
    std::fs::write(project.cache_dir().join("sdk"), "/no/such/sdk\n0.0\n").unwrap();
    let options = BuildOptions::default().with_linker(Linker::Ld);
    project.build_with(&program(&values()), &options);
    assert_eq!(project.run(), "21\n");
}

#[test]
fn parallel_and_sequential_compilation_produce_identical_caches() {
    let many: Vec<i64> = (0..120).collect();
    let source = program(&many);
    let sequential = Project::new();
    let parallel = Project::new();
    let one = sequential.build_with(&source, &BuildOptions::default().with_threads(1));
    let four = parallel.build_with(&source, &BuildOptions::default().with_threads(4));
    assert_eq!(one.compiled(), four.compiled());
    let pack =
        |project: &Project| std::fs::read(project.cache_dir().join("functions.pack")).unwrap();
    assert_eq!(pack(&sequential), pack(&parallel));
    assert_eq!(sequential.run(), parallel.run());
}

#[test]
fn the_first_build_checks_every_function() {
    let project = Project::new();
    let report = project.build(&program(&values()));
    assert_eq!(report.checked(), FUNCTIONS + 1);
}

#[test]
fn editing_only_a_comment_checks_no_function() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    let report = project.build(&format!("// a note\n{source}"));
    assert_eq!(report.checked(), 0);
    assert_eq!(project.run(), "21\n");
}

#[test]
fn editing_one_function_body_checks_only_that_function() {
    let project = Project::new();
    project.build(&program(&values()));
    let mut edited = values();
    edited[2] = 100;
    let report = project.build(&program(&edited));
    assert_eq!(report.checked(), 1);
    assert_eq!(project.run(), "118\n");
}

#[test]
fn changing_a_signature_checks_every_function_again() {
    let project = Project::new();
    let before = "func g() -> Int { return 1 }\nfunc h() { print(g()) }\nfunc main() { h() }\n";
    let after = "func g() -> Bool { return true }\nfunc h() { print(g()) }\nfunc main() { h() }\n";
    project.build(before);
    let report = project.build(after);
    assert_eq!(report.checked(), 3);
    assert_eq!(project.run(), "true\n");
}

#[test]
fn a_caller_broken_by_a_signature_change_is_still_reported() {
    let project = Project::new();
    let before = "func g() -> Int { return 1 }\nfunc h() -> Int { return g() }\nfunc main() { print(h()) }\n";
    let after = "func g() -> Bool { return true }\nfunc h() -> Int { return g() }\nfunc main() { print(h()) }\n";
    project.build(before);
    assert!(build_with(after, &project.output(), &BuildOptions::default()).is_err());
}

#[test]
fn a_corrupt_item_memo_falls_back_to_checking_everything() {
    let project = Project::new();
    let source = program(&values());
    project.build(&source);
    std::fs::write(project.cache_dir().join("items"), b"corrupt").unwrap();
    let report = project.build(&format!("// a note\n{source}"));
    assert_eq!(report.checked(), FUNCTIONS + 1);
    assert_eq!(report.compiled(), 0);
    assert_eq!(project.run(), "21\n");
}

#[test]
fn a_missing_main_is_reported_even_when_every_function_is_unchanged() {
    let project = Project::new();
    project.build("func f() { }\nfunc main() { f() }\n");
    let error = build_with(
        "func f() { }\n",
        &project.output(),
        &BuildOptions::default(),
    );
    assert!(error.is_err());
}

const GENERIC_PROGRAM: &str = "func id<T>(_ x: T) -> T { return x }\n\
    func a() -> Int { return id(1) }\n\
    func b() -> Int { return 2 }\n\
    func main() { print(a() + b() + id(10)) }\n";

#[test]
fn a_generic_instance_is_compiled_once_and_shared_by_its_callers() {
    let project = Project::new();
    let report = project.build(GENERIC_PROGRAM);
    assert_eq!(report.compiled(), 4);
    assert_eq!(project.run(), "13\n");
}

#[test]
fn editing_a_comment_reuses_the_generic_instances_without_checking() {
    let project = Project::new();
    project.build(GENERIC_PROGRAM);
    let report = project.build(&format!("// note\n{GENERIC_PROGRAM}"));
    assert_eq!(report.checked(), 0);
    assert_eq!(report.compiled(), 0);
    assert_eq!(report.reused(), 4);
}

#[test]
fn editing_an_unrelated_function_keeps_the_instances_of_unchanged_callers() {
    let project = Project::new();
    project.build(GENERIC_PROGRAM);
    let report = project.build(&GENERIC_PROGRAM.replace("return 2", "return 3"));
    assert_eq!(report.checked(), 1);
    assert_eq!(report.compiled(), 1);
    assert!(report.linked());
    assert_eq!(project.run(), "14\n");
}

#[test]
fn editing_a_generic_body_rechecks_everything_but_recompiles_only_what_changed() {
    let project = Project::new();
    project.build(GENERIC_PROGRAM);
    let edited = GENERIC_PROGRAM.replace("{ return x }", "{ let y = x\nreturn y }");
    let report = project.build(&edited);
    assert_eq!(report.checked(), 3);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "13\n");
}

#[test]
fn a_new_use_of_a_generic_function_adds_only_the_new_instance() {
    let project = Project::new();
    project.build(GENERIC_PROGRAM);
    let edited = GENERIC_PROGRAM.replace(
        "print(a() + b() + id(10))",
        "print(a() + b() + id(10))\nprint(id(true))",
    );
    let report = project.build(&edited);
    assert_eq!(report.compiled(), 2);
    assert_eq!(project.run(), "13\ntrue\n");
}

#[test]
fn generic_instances_are_dropped_from_the_cache_when_no_longer_used() {
    let project = Project::new();
    project.build(GENERIC_PROGRAM);
    project.build("func main() { print(1) }\n");
    assert_eq!(Cache::open(&project.cache_dir()).len(), 1);
}

fn counter_program(step: i64) -> String {
    format!(
        "class Counter {{\n  var n: Int\n  func bump() {{ self.n += {step} }}\n  func read() -> Int {{ return self.n }}\n}}\n\
         func other() -> Int {{ return 5 }}\n\
         func main() {{\n  let c = Counter(n: 0)\n  c.bump()\n  print(c.read() + other())\n}}\n"
    )
}

#[test]
fn editing_a_method_body_checks_and_compiles_only_that_method() {
    let project = Project::new();
    project.build(&counter_program(1));
    assert_eq!(project.run(), "6\n");
    let report = project.build(&counter_program(2));
    assert_eq!(report.checked(), 1);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "7\n");
}

#[test]
fn adding_a_method_checks_every_function_again() {
    let project = Project::new();
    project.build(&counter_program(1));
    let extended = counter_program(1).replace(
        "func read()",
        "func twice() -> Int { return self.n * 2 }\n  func read()",
    );
    let report = project.build(&extended);
    assert!(report.checked() > 1);
    assert_eq!(project.run(), "6\n");
}

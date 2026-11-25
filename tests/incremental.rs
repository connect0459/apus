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

fn box_program(offset: i64) -> String {
    format!(
        "struct Box<T> {{\n  var value: T\n  func get() -> T {{ return self.value }}\n}}\n\
         func plain() -> Int {{ return {offset} }}\n\
         func main() {{\n  let b = Box(value: 40)\n  print(b.get() + plain())\n}}\n"
    )
}

#[test]
fn editing_a_caller_of_a_generic_method_reuses_the_instance() {
    let project = Project::new();
    project.build(&box_program(2));
    assert_eq!(project.run(), "42\n");
    let report = project.build(&box_program(3));
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "43\n");
}

#[test]
fn editing_the_body_of_a_generic_method_recompiles_its_instances() {
    let project = Project::new();
    project.build(&box_program(2));
    let edited = box_program(2).replace("return self.value", "let v = self.value\n    return v");
    let report = project.build(&edited);
    assert!(report.compiled() >= 1);
    assert_eq!(project.run(), "42\n");
}

fn tagger_program(offset: i64) -> String {
    format!(
        "struct Tagger {{\n  var n: Int\n  func echo<T>(_ x: T) -> T {{ return x }}\n}}\n\
         func plain() -> Int {{ return {offset} }}\n\
         func main() {{\n  let t = Tagger(n: 1)\n  print(t.echo(40) + plain())\n}}\n"
    )
}

#[test]
fn editing_a_caller_of_a_generic_method_of_a_plain_type_reuses_the_instance() {
    let project = Project::new();
    project.build(&tagger_program(2));
    assert_eq!(project.run(), "42\n");
    let report = project.build(&tagger_program(3));
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "43\n");
}

fn zoo_program(dog: i64) -> String {
    format!(
        "class Animal {{\n  var legs: Int\n  func sound() -> Int {{ return 1 }}\n}}\n\
         class Dog: Animal {{\n  override func sound() -> Int {{ return {dog} }}\n}}\n\
         func hear(_ a: Animal) -> Int {{ return a.sound() }}\n\
         func main() {{\n  print(hear(Animal(legs: 4)) + hear(Dog(legs: 4)))\n}}\n"
    )
}

#[test]
fn editing_an_override_body_checks_and_compiles_only_that_override() {
    let project = Project::new();
    project.build(&zoo_program(2));
    assert_eq!(project.run(), "3\n");
    let report = project.build(&zoo_program(5));
    assert_eq!(report.checked(), 1);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "6\n");
}

#[test]
fn adding_an_override_checks_every_function_again() {
    let project = Project::new();
    project.build(&zoo_program(2));
    let extended = format!(
        "{}class Cat: Animal {{\n  override func sound() -> Int {{ return 7 }}\n}}\n",
        zoo_program(2)
    );
    let report = project.build(&extended);
    assert!(report.checked() > 1);
    assert_eq!(project.run(), "3\n");
}

fn tracked_program(body: i64) -> String {
    format!(
        "class Tracked {{\n  var id: Int\n  deinit {{ print(0 - self.id) }}\n}}\n\
         func other() -> Int {{ return {body} }}\n\
         func main() {{\n  let t = Tracked(id: 1)\n  print(other())\n}}\n"
    )
}

#[test]
fn editing_a_function_that_never_touches_a_class_leaves_the_class_entries_cached() {
    let project = Project::new();
    project.build(&tracked_program(5));
    assert_eq!(project.run(), "5\n-1\n");
    let report = project.build(&tracked_program(6));
    assert_eq!(report.checked(), 1);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "6\n-1\n");
}

#[test]
fn editing_a_deinit_body_compiles_only_that_deinit() {
    let project = Project::new();
    project.build(&tracked_program(5));
    let edited = tracked_program(5).replace("0 - self.id", "self.id");
    let report = project.build(&edited);
    assert_eq!(report.checked(), 1);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "5\n1\n");
}

const SHAPE_PROGRAM: &str = "protocol Shape {\n  func area() -> Int\n}\nstruct Square: Shape {\n  var side: Int\n  func area() -> Int { return side * side }\n}\nfunc unrelated() -> Int { return 40 }\nfunc total(_ s: Shape) -> Int { return s.area() }\nfunc main() {\n  print(total(Square(side: 2)) + unrelated())\n}\n";

#[test]
fn a_new_conformer_recompiles_the_functions_that_call_a_requirement_through_the_protocol() {
    let project = Project::new();
    project.build(SHAPE_PROGRAM);
    assert_eq!(project.run(), "44\n");
    let with_conformer = format!(
        "{SHAPE_PROGRAM}struct Rect: Shape {{\n  var w: Int\n  var h: Int\n  func area() -> Int {{ return w * h }}\n}}\n"
    );
    let report = project.build(&with_conformer);
    assert!(report.compiled() >= 1);
    assert!(report.reused() >= 1, "unrelated code must stay cached");
    assert_eq!(project.run(), "44\n");
    let used = with_conformer.replace("Square(side: 2)", "Rect(w: 3, h: 4)");
    project.build(&used);
    assert_eq!(project.run(), "52\n");
}

#[test]
fn editing_an_unrelated_function_keeps_the_protocol_dispatch_cached() {
    let project = Project::new();
    project.build(SHAPE_PROGRAM);
    let edited = SHAPE_PROGRAM.replace("return 40", "return 41");
    let report = project.build(&edited);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "45\n");
}

#[test]
fn deep_expressions_compile_on_the_parallel_workers() {
    let project = Project::new();
    let chain = vec!["1"; 6000].join(" + ");
    let mut source = String::new();
    for index in 0..8 {
        source.push_str(&format!("func f{index}() -> Int {{ return {chain} }}\n"));
    }
    source
        .push_str("func main() { print(f0() + f1() + f2() + f3() + f4() + f5() + f6() + f7()) }\n");
    let report = project.build_with(&source, &BuildOptions::default().with_threads(4));
    assert_eq!(report.compiled(), 9);
    assert_eq!(project.run(), "48000\n");
}

const GENERIC_DISPATCH: &str = "class Base<T> {\n  var value: T\n  func describe() -> Int { return 1 }\n}\nclass A: Base<Int> {\n  override func describe() -> Int { return 7 }\n}\nfunc use(_ b: Base<Int>) -> Int { return b.describe() }\nfunc unrelated() -> Int { return 40 }\nfunc main() {\n  print(use(A(value: 0)) + unrelated())\n}\n";

#[test]
fn a_new_subclass_of_a_generic_class_dispatches_correctly_through_cached_callers() {
    let project = Project::new();
    project.build(GENERIC_DISPATCH);
    assert_eq!(project.run(), "47\n");
    let with_subclass = GENERIC_DISPATCH
        .replace(
            "func use(",
            "class B<T>: Base<T> {\n  override func describe() -> Int { return 8 }\n}\nfunc use(",
        )
        .replace(
            "print(use(A(value: 0)) + unrelated())",
            "print(use(A(value: 0)) + use(B<Int>(value: 0)) + unrelated())",
        );
    let report = project.build(&with_subclass);
    assert!(report.reused() >= 1, "the callers must stay cached");
    assert_eq!(project.run(), "55\n");
    let changed = with_subclass.replace("return 8", "return 9");
    project.build(&changed);
    assert_eq!(project.run(), "56\n");
}

#[test]
fn editing_an_override_of_a_generic_hierarchy_recompiles_only_that_method() {
    let project = Project::new();
    project.build(GENERIC_DISPATCH);
    let edited = GENERIC_DISPATCH.replace("return 7", "return 70");
    let report = project.build(&edited);
    assert_eq!(report.compiled(), 1);
    assert_eq!(project.run(), "110\n");
}

const GENERIC_DEINIT: &str = "class Counted<T> {\n  var item: T\n  var id: Int\n  deinit { print(0 - id) }\n}\nfunc unrelated() -> Int { return 40 }\nfunc main() {\n  let c = Counted<Int>(item: 1, id: 2)\n  print(unrelated())\n}\n";

#[test]
fn editing_the_deinit_of_a_generic_class_recompiles_only_its_instances() {
    let project = Project::new();
    project.build(GENERIC_DEINIT);
    assert_eq!(project.run(), "40\n-2\n");
    let edited = GENERIC_DEINIT.replace("print(0 - id)", "print(100 - id)");
    let report = project.build(&edited);
    assert!(report.reused() >= 1, "unrelated code must stay cached");
    assert_eq!(project.run(), "40\n98\n");
}

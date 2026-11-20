use apus::driver::build;
use std::process::Command;

fn run(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("prog");
    build(source, &exe).unwrap();
    let output = Command::new(&exe).output().unwrap();
    assert!(
        output.status.success(),
        "program exited with {:?}",
        output.status
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn printing_an_integer_writes_it_followed_by_a_newline() {
    assert_eq!(run("func main() { print(42) }"), "42\n");
}

#[test]
fn an_empty_main_builds_a_program_that_prints_nothing() {
    assert_eq!(run("func main() { }"), "");
}

#[test]
fn a_program_without_main_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let err = build("func other() { }", &dir.path().join("prog")).unwrap_err();
    assert!(err.message().contains("main"), "{}", err.message());
}

fn run_expecting_a_crash(source: &str) {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("prog");
    build(source, &exe).unwrap();
    let output = Command::new(&exe).output().unwrap();
    assert!(!output.status.success(), "program should have trapped");
}

fn main_printing(body: &str) -> String {
    run(&format!("func main() {{\n{body}\n}}"))
}

#[test]
fn arithmetic_follows_operator_precedence() {
    assert_eq!(
        main_printing("print(1 + 2 * 3)\nprint((1 + 2) * 3)\nprint(7 - 2 - 1)"),
        "7\n9\n4\n"
    );
}

#[test]
fn division_and_remainder_truncate_toward_zero() {
    assert_eq!(
        main_printing("print(7 / 2)\nprint(-7 / 2)\nprint(7 % 3)\nprint(-7 % 3)"),
        "3\n-3\n1\n-1\n"
    );
}

#[test]
fn unary_minus_negates_its_operand() {
    assert_eq!(
        main_printing("let a = 5\nprint(-a)\nprint(- -a)"),
        "-5\n5\n"
    );
}

#[test]
fn variables_hold_values_and_compound_assignment_updates_them() {
    assert_eq!(
        main_printing("var s = 1\ns += 4\ns *= 3\ns -= 5\ns /= 2\ns %= 4\nprint(s)"),
        "1\n"
    );
}

#[test]
fn if_else_runs_only_the_branch_whose_condition_holds() {
    assert_eq!(
        main_printing(
            "let a = 3\nif a > 5 { print(1) } else if a > 2 { print(2) } else { print(3) }\nif a == 3 { print(4) }"
        ),
        "2\n4\n"
    );
}

#[test]
fn while_repeats_until_its_condition_fails() {
    assert_eq!(
        main_printing("var i = 0\nvar s = 0\nwhile i < 5 { s += i\ni += 1 }\nprint(s)"),
        "10\n"
    );
}

#[test]
fn for_over_a_half_open_range_excludes_the_upper_bound() {
    assert_eq!(
        main_printing("var s = 0\nfor k in 0..<5 { s += k }\nprint(s)"),
        "10\n"
    );
}

#[test]
fn for_over_a_closed_range_includes_the_upper_bound() {
    assert_eq!(
        main_printing("var s = 0\nfor k in 1...3 { s += k }\nprint(s)"),
        "6\n"
    );
}

#[test]
fn an_empty_range_runs_its_body_zero_times() {
    assert_eq!(
        main_printing("for k in 3..<3 { print(k) }\nfor k in 5...4 { print(k) }"),
        ""
    );
}

#[test]
fn functions_take_labeled_and_unlabeled_arguments_and_return_values() {
    let source = "func add(_ a: Int, to b: Int) -> Int { return a + b }\n\
                  func main() { print(add(2, to: 3)) }";
    assert_eq!(run(source), "5\n");
}

#[test]
fn recursion_works() {
    let source = "func fib(_ n: Int) -> Int {\n  if n < 2 { return n }\n  return fib(n - 1) + fib(n - 2)\n}\n\
                  func main() { print(fib(15)) }";
    assert_eq!(run(source), "610\n");
}

#[test]
fn functions_may_be_used_before_they_are_declared() {
    let source = "func main() { print(later()) }\nfunc later() -> Int { return 9 }";
    assert_eq!(run(source), "9\n");
}

#[test]
fn return_inside_a_loop_leaves_the_function() {
    let source = "func first_over(_ limit: Int) -> Int {\n  var i = 0\n  while true {\n    if i * i > limit { return i }\n    i += 1\n  }\n  return 0\n}\n\
                  func main() { print(first_over(50)) }";
    assert_eq!(run(source), "8\n");
}

#[test]
fn a_void_function_may_return_early() {
    let source = "func show(_ n: Int) {\n  if n < 0 { return }\n  print(n)\n}\n\
                  func main() { show(-1)\nshow(2) }";
    assert_eq!(run(source), "2\n");
}

#[test]
fn booleans_print_as_true_or_false() {
    assert_eq!(
        main_printing("print(true)\nprint(1 > 2)\nprint(!false)"),
        "true\nfalse\ntrue\n"
    );
}

#[test]
fn comparisons_cover_all_six_operators() {
    assert_eq!(
        main_printing(
            "print(1 < 2)\nprint(2 <= 2)\nprint(3 > 4)\nprint(4 >= 5)\nprint(5 == 5)\nprint(5 != 5)"
        ),
        "true\ntrue\nfalse\nfalse\ntrue\nfalse\n"
    );
}

#[test]
fn logical_operators_short_circuit() {
    let source = "func boom() -> Bool { print(99)\nreturn true }\n\
                  func main() {\n  print(false && boom())\n  print(true || boom())\n  print(true && boom())\n}";
    assert_eq!(run(source), "false\ntrue\n99\ntrue\n");
}

#[test]
fn integer_overflow_traps() {
    run_expecting_a_crash("func main() {\n  var x = 9223372036854775807\n  x += 1\n  print(x)\n}");
}

#[test]
fn multiplication_overflow_traps() {
    run_expecting_a_crash("func main() {\n  let x = 4611686018427387904\n  print(x * 4)\n}");
}

#[test]
fn division_by_zero_traps() {
    run_expecting_a_crash("func main() {\n  let z = 0\n  print(1 / z)\n}");
}

#[test]
fn negating_the_smallest_integer_traps() {
    run_expecting_a_crash("func main() {\n  let m = -9223372036854775807 - 1\n  print(-m)\n}");
}

#[test]
fn printing_a_string_literal_writes_it_followed_by_a_newline() {
    assert_eq!(main_printing("print(\"hello\")"), "hello\n");
}

#[test]
fn string_literals_keep_escapes_and_non_ascii_text() {
    assert_eq!(
        main_printing("print(\"a\\tb\")\nprint(\"こんにちは\")"),
        "a\tb\nこんにちは\n"
    );
}

#[test]
fn strings_can_be_stored_passed_and_returned() {
    let source = "func pick(_ flag: Bool, _ a: String, _ b: String) -> String {\n  if flag { return a }\n  return b\n}\n\
                  func main() {\n  let s: String = \"first\"\n  print(pick(true, s, \"second\"))\n  print(pick(false, s, \"second\"))\n}";
    assert_eq!(run(source), "first\nsecond\n");
}

#[test]
fn identical_string_literals_in_different_functions_build_together() {
    let source =
        "func a() { print(\"same\") }\nfunc b() { print(\"same\") }\nfunc main() { a()\nb() }";
    assert_eq!(run(source), "same\nsame\n");
}

const POINT: &str = "struct P { var x: Int; var y: Int }\n";

fn with_point(rest: &str) -> String {
    run(&format!("{POINT}{rest}"))
}

#[test]
fn a_struct_is_built_with_its_memberwise_initializer_and_read_by_member() {
    assert_eq!(
        with_point("func main() {\n  let p = P(x: 1, y: 2)\n  print(p.x + p.y * 10)\n}"),
        "21\n"
    );
}

#[test]
fn members_of_a_var_struct_can_be_assigned() {
    assert_eq!(
        with_point(
            "func main() {\n  var p = P(x: 1, y: 2)\n  p.x = 5\n  p.y += 3\n  p.y *= 2\n  print(p.x)\n  print(p.y)\n}"
        ),
        "5\n10\n"
    );
}

#[test]
fn assigning_a_struct_copies_it() {
    assert_eq!(
        with_point(
            "func main() {\n  var a = P(x: 1, y: 2)\n  var b = a\n  b.x = 9\n  print(a.x)\n  print(b.x)\n  a = b\n  b.x = 7\n  print(a.x)\n}"
        ),
        "1\n9\n9\n"
    );
}

#[test]
fn structs_are_passed_to_and_returned_from_functions() {
    let source = "func swapped(_ p: P) -> P { return P(x: p.y, y: p.x) }\n\
                  func main() {\n  let p = swapped(P(x: 1, y: 2))\n  print(p.x)\n  print(p.y)\n}";
    assert_eq!(with_point(source), "2\n1\n");
}

#[test]
fn a_struct_result_may_overwrite_the_variable_it_was_computed_from() {
    let source = "func swapped(_ p: P) -> P { return P(x: p.y, y: p.x) }\n\
                  func main() {\n  var p = P(x: 1, y: 2)\n  p = swapped(p)\n  print(p.x)\n  print(p.y)\n}";
    assert_eq!(with_point(source), "2\n1\n");
}

#[test]
fn a_member_of_a_call_result_can_be_read_directly() {
    let source = "func make(_ n: Int) -> P { return P(x: n, y: n * 2) }\n\
                  func main() { print(make(4).y) }";
    assert_eq!(with_point(source), "8\n");
}

#[test]
fn a_callee_does_not_change_the_callers_struct() {
    let source = "func bump(_ p: P) -> P {\n  var q = p\n  q.x += 100\n  return q\n}\n\
                  func main() {\n  let a = P(x: 1, y: 2)\n  let b = bump(a)\n  print(a.x)\n  print(b.x)\n}";
    assert_eq!(with_point(source), "1\n101\n");
}

#[test]
fn struct_members_may_be_booleans_and_strings() {
    let source = "struct Item { var name: String; var on: Bool; var count: Int }\n\
                  func main() {\n  let i = Item(name: \"pen\", on: true, count: 3)\n  print(i.name)\n  print(i.on)\n  print(i.count)\n}";
    assert_eq!(run(source), "pen\ntrue\n3\n");
}

#[test]
fn structs_may_be_created_inside_loops() {
    let source = "func main() {\n  var s = 0\n  for k in 0..<4 {\n    let p = P(x: k, y: k * k)\n    s += p.x + p.y\n  }\n  print(s)\n}";
    assert_eq!(with_point(source), "20\n");
}

#[test]
fn struct_fields_use_overflow_checked_arithmetic() {
    run_expecting_a_crash(&format!(
        "{POINT}func main() {{\n  var p = P(x: 9223372036854775807, y: 0)\n  p.x += 1\n  print(p.x)\n}}"
    ));
}

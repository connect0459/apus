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

const COLOR: &str = "enum Color { case red, green, blue }\n";

fn with_color(body: &str) -> String {
    format!("{COLOR}{body}")
}

#[test]
fn a_switch_runs_the_body_of_the_matching_case() {
    let source = with_color(
        "func name(_ c: Color) -> Int {\nswitch c {\ncase .red: return 1\ncase .green: return 2\ncase .blue: return 3\n}\n}\n\
         func main() { print(name(Color.red))\nprint(name(Color.green))\nprint(name(Color.blue)) }",
    );
    assert_eq!(run(&source), "1\n2\n3\n");
}

#[test]
fn a_case_with_several_patterns_and_a_default_both_match() {
    let source = with_color(
        "func warm(_ c: Color) -> Bool {\nswitch c {\ncase .red, .green: return true\ndefault: return false\n}\n}\n\
         func main() { print(warm(Color.green))\nprint(warm(Color.blue)) }",
    );
    assert_eq!(run(&source), "true\nfalse\n");
}

#[test]
fn a_switch_evaluates_its_subject_only_once() {
    let source = with_color(
        "func next() -> Color { print(9)\nreturn Color.green }\n\
         func main() {\nswitch next() {\ncase .red: print(1)\ncase .green: print(2)\ncase .blue: print(3)\n}\n}",
    );
    assert_eq!(run(&source), "9\n2\n");
}

#[test]
fn enum_values_are_stored_in_variables_and_compared() {
    let source = with_color(
        "func main() {\nvar c = Color.red\nprint(c == Color.red)\nc = Color.blue\nprint(c == Color.red)\nprint(c != Color.red)\n}",
    );
    assert_eq!(run(&source), "true\nfalse\ntrue\n");
}

#[test]
fn an_enum_can_be_a_struct_field_and_a_parameter() {
    let source = with_color(
        "struct Pixel { var color: Color }\n\
         func isBlue(_ p: Pixel) -> Bool { return p.color == Color.blue }\n\
         func main() { var p = Pixel(color: Color.green)\nprint(isBlue(p))\np.color = Color.blue\nprint(isBlue(p)) }",
    );
    assert_eq!(run(&source), "false\ntrue\n");
}

#[test]
fn a_non_exhaustive_switch_fails_the_build() {
    let dir = tempfile::tempdir().unwrap();
    let source = with_color("func main() {\nswitch Color.red {\ncase .red: print(1)\n}\n}");
    let err = build(&source, &dir.path().join("prog")).unwrap_err();
    assert!(err.message().contains("exhaustive"), "{}", err.message());
}

const SHAPE: &str = "enum Shape { case circle(Int), rect(Int, Int), label(String, Bool), empty }\n\
    func describe(_ s: Shape) {\n\
    switch s {\n\
    case .circle(let r): print(r)\n\
    case .rect(let w, let h): print(w * h)\n\
    case .label(let text, let flag): print(text)\nprint(flag)\n\
    case .empty: print(-1)\n\
    }\n\
    }\n";

fn with_shape(body: &str) -> String {
    format!("{SHAPE}func main() {{\n{body}\n}}")
}

#[test]
fn a_switch_binds_the_values_carried_by_the_matching_case() {
    let source = with_shape(
        "describe(Shape.circle(5))\ndescribe(Shape.rect(3, 4))\ndescribe(Shape.label(\"hi\", true))\ndescribe(Shape.empty)",
    );
    assert_eq!(run(&source), "5\n12\nhi\ntrue\n-1\n");
}

#[test]
fn a_payload_enum_is_copied_by_value() {
    let source = with_shape(
        "var a = Shape.circle(1)\nlet b = a\na = Shape.circle(2)\ndescribe(b)\ndescribe(a)",
    );
    assert_eq!(run(&source), "1\n2\n");
}

#[test]
fn a_payload_enum_can_be_returned_from_a_function() {
    let source = format!(
        "{SHAPE}func grow(_ s: Shape) -> Shape {{\nswitch s {{\ncase .circle(let r): return Shape.circle(r + 1)\ndefault: return s\n}}\n}}\n\
         func main() {{ describe(grow(Shape.circle(9)))\ndescribe(grow(Shape.empty)) }}"
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_case_without_bindings_matches_whatever_it_carries() {
    let source =
        with_shape("switch Shape.rect(1, 2) {\ncase .rect: print(1)\ndefault: print(0)\n}");
    assert_eq!(run(&source), "1\n");
}

#[test]
fn a_payload_may_be_an_enum_without_values() {
    let source = "enum Color { case red, green }\nenum Paint { case solid(Color), none }\n\
        func main() {\nswitch Paint.solid(Color.green) {\ncase .solid(let c): print(c == Color.green)\ncase .none: print(false)\n}\n}";
    assert_eq!(run(source), "true\n");
}

#[test]
fn a_switch_with_a_default_handles_the_unlisted_payload_cases() {
    let source = with_shape(
        "switch Shape.label(\"a\", false) {\ncase .circle(let r): print(r)\ndefault: print(7)\n}",
    );
    assert_eq!(run(&source), "7\n");
}

#[test]
fn a_generic_function_is_instantiated_for_each_type_it_is_called_with() {
    let source = "func id<T>(_ x: T) -> T { return x }\n\
        func main() { print(id(41) + 1)\nprint(id(true))\nprint(id(\"hi\")) }";
    assert_eq!(run(source), "42\ntrue\nhi\n");
}

#[test]
fn a_generic_function_may_take_several_parameters_and_return_one_of_them() {
    let source = "func second<A, B>(_ a: A, _ b: B) -> B { return b }\n\
        func main() { print(second(1, \"x\"))\nprint(second(\"y\", 2)) }";
    assert_eq!(run(source), "x\n2\n");
}

#[test]
fn a_generic_function_may_call_another_generic_function() {
    let source = "func id<T>(_ x: T) -> T { return x }\n\
        func twice<T>(_ x: T) -> T { return id(id(x)) }\n\
        func main() { print(twice(7)) }";
    assert_eq!(run(source), "7\n");
}

#[test]
fn a_generic_function_may_copy_struct_values() {
    let source = "struct P { var x: Int }\n\
        func id<T>(_ v: T) -> T { let copy = v\nreturn copy }\n\
        func main() { let p = id(P(x: 5))\nprint(p.x) }";
    assert_eq!(run(source), "5\n");
}

#[test]
fn a_recursive_generic_function_instantiates_once() {
    let source = "func count<T>(_ x: T, _ n: Int) -> Int {\nif n == 0 { return 0 }\nreturn 1 + count(x, n - 1)\n}\n\
        func main() { print(count(true, 4)) }";
    assert_eq!(run(source), "4\n");
}

#[test]
fn a_generic_struct_stores_values_of_its_type_argument() {
    let source = "struct Box<T> { var value: T }\n\
        func main() {\nvar a = Box(value: 3)\nvar b = Box(value: \"s\")\nprint(a.value)\nprint(b.value)\na.value = 4\nprint(a.value)\n}";
    assert_eq!(run(source), "3\ns\n4\n");
}

#[test]
fn a_generic_struct_can_be_named_with_explicit_type_arguments() {
    let source = "struct Box<T> { var value: T }\n\
        func unwrap(_ b: Box<Int>) -> Int { return b.value }\n\
        func main() { let b: Box<Int> = Box<Int>(value: 9)\nprint(unwrap(b)) }";
    assert_eq!(run(source), "9\n");
}

#[test]
fn a_generic_function_can_take_and_build_a_generic_struct() {
    let source = "struct Box<T> { var value: T }\n\
        func wrap<T>(_ x: T) -> Box<T> { return Box(value: x) }\n\
        func open<T>(_ b: Box<T>) -> T { return b.value }\n\
        func main() { print(open(wrap(8)))\nprint(open(wrap(\"w\"))) }";
    assert_eq!(run(source), "8\nw\n");
}

#[test]
fn a_generic_enum_carries_a_value_of_its_type_argument() {
    let source = "enum Maybe<T> { case some(T), none }\n\
        func show(_ m: Maybe<Int>) {\nswitch m {\ncase .some(let v): print(v)\ncase .none: print(-1)\n}\n}\n\
        func main() { show(Maybe.some(5))\nshow(Maybe<Int>.none) }";
    assert_eq!(run(source), "5\n-1\n");
}

#[test]
fn a_generic_function_can_switch_on_a_generic_enum() {
    let source = "enum Maybe<T> { case some(T), none }\n\
        func orElse<T>(_ m: Maybe<T>, _ fallback: T) -> T {\nswitch m {\ncase .some(let v): return v\ncase .none: return fallback\n}\n}\n\
        func main() { print(orElse(Maybe.some(1), 0))\nprint(orElse(Maybe<String>.none, \"none\")) }";
    assert_eq!(run(source), "1\nnone\n");
}

#[test]
fn a_generic_enum_may_hold_a_pair_of_type_arguments() {
    let source = "enum Either<L, R> { case left(L), right(R) }\n\
        func main() {\nlet e: Either<Int, String> = Either.right(\"r\")\nswitch e {\ncase .left(let n): print(n)\ncase .right(let s): print(s)\n}\n}";
    assert_eq!(run(source), "r\n");
}

const COUNTER: &str = "class Counter { var count: Int; let label: String }\n";

fn with_counter(body: &str) -> String {
    format!("{COUNTER}{body}")
}

fn build_error(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    build(source, &dir.path().join("prog"))
        .unwrap_err()
        .message()
        .to_string()
}

#[test]
fn a_class_instance_exposes_the_members_it_was_created_with() {
    let source = with_counter(
        "func main() {\n  let c = Counter(count: 3, label: \"hits\")\n  print(c.count)\n  print(c.label)\n}",
    );
    assert_eq!(run(&source), "3\nhits\n");
}

#[test]
fn copying_a_class_reference_shares_the_instance() {
    let source = with_counter(
        "func main() {\n  let a = Counter(count: 1, label: \"a\")\n  let b = a\n  b.count = 5\n  print(a.count)\n}",
    );
    assert_eq!(run(&source), "5\n");
}

#[test]
fn a_function_can_mutate_the_instance_behind_its_parameter() {
    let source = with_counter(
        "func bump(_ c: Counter) { c.count += 1 }\n\
         func main() {\n  let c = Counter(count: 1, label: \"c\")\n  bump(c)\n  bump(c)\n  print(c.count)\n}",
    );
    assert_eq!(run(&source), "3\n");
}

#[test]
fn a_function_can_return_a_new_class_instance() {
    let source = with_counter(
        "func make(_ n: Int) -> Counter { return Counter(count: n, label: \"made\") }\n\
         func main() {\n  let c = make(7)\n  print(c.count)\n}",
    );
    assert_eq!(run(&source), "7\n");
}

#[test]
fn a_reassigned_class_variable_points_at_the_new_instance() {
    let source = with_counter(
        "func main() {\n  let a = Counter(count: 1, label: \"a\")\n  var b = a\n  b = Counter(count: 2, label: \"b\")\n  b.count += 10\n  print(a.count)\n  print(b.count)\n}",
    );
    assert_eq!(run(&source), "1\n12\n");
}

#[test]
fn a_class_may_hold_another_class_and_be_held_by_a_struct() {
    let source = format!(
        "{COUNTER}class Pair {{ let first: Counter; let second: Counter }}\n\
         struct Holder {{ var counter: Counter }}\n\
         func main() {{\n  let c = Counter(count: 1, label: \"c\")\n  let p = Pair(first: c, second: c)\n  let f = p.first\n  f.count = 4\n  let h = Holder(counter: p.second)\n  print(h.counter.count)\n}}"
    );
    assert_eq!(run(&source), "4\n");
}

#[test]
fn a_generic_struct_can_hold_a_class_instance() {
    let source = format!(
        "{COUNTER}struct Box<T> {{ var value: T }}\n\
         func main() {{\n  let b = Box(value: Counter(count: 9, label: \"b\"))\n  print(b.value.count)\n}}"
    );
    assert_eq!(run(&source), "9\n");
}

#[test]
fn a_generic_class_is_instantiated_per_type_argument() {
    let source = "class Cell<T> { var value: T }\n\
         func main() {\n  let a = Cell(value: 1)\n  let b = Cell(value: true)\n  a.value += 1\n  print(a.value)\n  print(b.value)\n}";
    assert_eq!(run(source), "2\ntrue\n");
}

#[test]
fn a_constant_member_of_a_class_cannot_be_assigned() {
    let source = with_counter(
        "func main() {\n  let c = Counter(count: 1, label: \"c\")\n  c.label = \"d\"\n}",
    );
    assert!(build_error(&source).contains("constant"));
}

#[test]
fn a_class_instance_cannot_be_printed_or_compared() {
    let print =
        with_counter("func main() {\n  let c = Counter(count: 1, label: \"c\")\n  print(c)\n}");
    assert!(build_error(&print).contains("cannot print"));
    let compare = with_counter(
        "func main() {\n  let a = Counter(count: 1, label: \"a\")\n  print(a == a)\n}",
    );
    assert!(build_error(&compare).contains("equality"));
}

const GAUGE: &str = "class Gauge {\n  var level: Int\n  func raise(by amount: Int) { self.level += amount }\n  func read() -> Int { return self.level }\n}\n";

#[test]
fn a_class_method_changes_the_instance_it_is_called_on() {
    let source = format!(
        "{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  g.raise(by: 4)\n  g.raise(by: 5)\n  print(g.read())\n}}"
    );
    assert_eq!(run(&source), "10\n");
}

#[test]
fn a_method_can_call_another_method_through_self() {
    let source = "class Gauge {\n  var level: Int\n  func read() -> Int { return self.level }\n  func double() -> Int { return self.read() * 2 }\n}\n\
         func main() { print(Gauge(level: 21).double()) }";
    assert_eq!(run(source), "42\n");
}

const VECTOR: &str = "struct V {\n  var x: Int\n  var y: Int\n  func sum() -> Int { return self.x + self.y }\n  func scaled(by k: Int) -> V { return V(x: self.x * k, y: self.y * k) }\n  mutating func shift(dx: Int, dy: Int) {\n    self.x += dx\n    self.y += dy\n  }\n}\n";

#[test]
fn a_struct_method_reads_the_value_it_is_called_on() {
    let source = format!(
        "{VECTOR}func main() {{\n  let v = V(x: 3, y: 4)\n  print(v.sum())\n  print(v.scaled(by: 2).sum())\n}}"
    );
    assert_eq!(run(&source), "7\n14\n");
}

#[test]
fn a_mutating_method_changes_the_variable_it_is_called_on() {
    let source = format!(
        "{VECTOR}func main() {{\n  var v = V(x: 1, y: 1)\n  v.shift(dx: 2, dy: 3)\n  v.shift(dx: 10, dy: 10)\n  print(v.x)\n  print(v.y)\n}}"
    );
    assert_eq!(run(&source), "13\n14\n");
}

#[test]
fn a_mutating_method_does_not_change_a_copy_made_before_the_call() {
    let source = format!(
        "{VECTOR}func main() {{\n  var v = V(x: 1, y: 1)\n  let before = v\n  v.shift(dx: 5, dy: 5)\n  print(before.x)\n  print(v.x)\n}}"
    );
    assert_eq!(run(&source), "1\n6\n");
}

#[test]
fn a_mutating_method_may_call_another_mutating_method_on_self() {
    let source = "struct C {\n  var n: Int\n  mutating func inc() { self.n += 1 }\n  mutating func twice() {\n    self.inc()\n    self.inc()\n  }\n}\n\
         func main() {\n  var c = C(n: 0)\n  c.twice()\n  print(c.n)\n}";
    assert_eq!(run(source), "2\n");
}

#[test]
fn a_mutating_method_can_replace_self_wholesale() {
    let source = "struct C {\n  var n: Int\n  mutating func reset() { self = C(n: 0) }\n}\n\
         func main() {\n  var c = C(n: 9)\n  c.reset()\n  print(c.n)\n}";
    assert_eq!(run(source), "0\n");
}

#[test]
fn an_enum_method_switches_on_self() {
    let source = "enum Light {\n  case red, green\n  func next() -> Light {\n    switch self {\n    case .red: return Light.green\n    case .green: return Light.red\n    }\n  }\n}\n\
         func main() {\n  let l = Light.red\n  print(l.next() == Light.green)\n}";
    assert_eq!(run(source), "true\n");
}

#[test]
fn a_mutating_method_of_an_enum_with_payloads_replaces_self() {
    let source = "enum Shape {\n  case empty\n  case circle(Int)\n  mutating func grow() {\n    switch self {\n    case .empty: self = Shape.circle(1)\n    case .circle(let r): self = Shape.circle(r + 1)\n    }\n  }\n  func size() -> Int {\n    switch self {\n    case .empty: return 0\n    case .circle(let r): return r\n    }\n  }\n}\n\
         func main() {\n  var s = Shape.empty\n  s.grow()\n  s.grow()\n  print(s.size())\n}";
    assert_eq!(run(source), "2\n");
}

#[test]
fn a_method_can_take_and_return_class_instances() {
    let source = "class Box {\n  var v: Int\n  func with(other: Box) -> Box { return Box(v: self.v + other.v) }\n}\n\
         func main() {\n  let a = Box(v: 1)\n  let b = Box(v: 2)\n  print(a.with(other: b).v)\n}";
    assert_eq!(run(source), "3\n");
}

#[test]
fn methods_of_different_types_may_share_a_name() {
    let source = "struct A { var x: Int\n  func get() -> Int { return self.x } }\n\
         struct B { var x: Int\n  func get() -> Int { return self.x + 100 } }\n\
         func main() { print(A(x: 1).get())\n print(B(x: 1).get()) }";
    assert_eq!(run(source), "1\n101\n");
}

#[test]
fn a_non_mutating_method_cannot_assign_to_self() {
    let source = "struct C { var n: Int\n  func bad() { self.n = 1 } }\nfunc main() { }";
    assert!(build_error(source).contains("constant"));
}

#[test]
fn a_mutating_method_cannot_be_called_on_a_constant() {
    let source =
        format!("{VECTOR}func main() {{\n  let v = V(x: 1, y: 1)\n  v.shift(dx: 1, dy: 1)\n}}");
    assert!(build_error(&source).contains("mutating"));
}

#[test]
fn a_mutating_method_cannot_be_called_on_a_temporary() {
    let source = format!("{VECTOR}func main() {{\n  V(x: 1, y: 1).shift(dx: 1, dy: 1)\n}}");
    assert!(build_error(&source).contains("mutating"));
}

#[test]
fn a_class_method_cannot_be_mutating() {
    let source = "class C { var n: Int\n  mutating func bad() { } }\nfunc main() { }";
    assert!(build_error(source).contains("class"));
}

const LIGHT: &str = "enum Light {\n  case red, green, blue\n  mutating func next() {\n    switch self {\n    case .red: self = Light.green\n    case .green: self = Light.blue\n    case .blue: self = Light.red\n    }\n  }\n}\n";

#[test]
fn a_mutating_method_of_an_enum_without_payloads_replaces_the_variable() {
    let source = format!(
        "{LIGHT}func main() {{\n  var l = Light.red\n  l.next()\n  print(l == Light.green)\n  l.next()\n  l.next()\n  print(l == Light.red)\n}}"
    );
    assert_eq!(run(&source), "true\ntrue\n");
}

#[test]
fn a_mutating_enum_method_does_not_change_a_copy_made_before_the_call() {
    let source = format!(
        "{LIGHT}func main() {{\n  var l = Light.red\n  let before = l\n  l.next()\n  print(before == Light.red)\n}}"
    );
    assert_eq!(run(&source), "true\n");
}

#[test]
fn a_mutating_enum_method_may_call_another_on_self() {
    let source = "enum Light {\n  case red, green, blue\n  mutating func next() {\n    switch self {\n    case .red: self = Light.green\n    case .green: self = Light.blue\n    case .blue: self = Light.red\n    }\n  }\n  mutating func skip() {\n    self.next()\n    next()\n  }\n}\n\
         func main() {\n  var l = Light.red\n  l.skip()\n  print(l == Light.blue)\n}";
    assert_eq!(run(source), "true\n");
}

#[test]
fn a_mutating_enum_method_writes_back_when_it_returns_early() {
    let source = "enum Flag {\n  case off, on\n  mutating func turnOn(force: Bool) {\n    self = Flag.on\n    if force { return }\n    self = Flag.off\n  }\n}\n\
         func main() {\n  var f = Flag.off\n  f.turnOn(force: true)\n  print(f == Flag.on)\n  f.turnOn(force: false)\n  print(f == Flag.on)\n}";
    assert_eq!(run(source), "true\nfalse\n");
}

#[test]
fn a_mutating_enum_method_may_take_arguments_and_leave_a_result_unused() {
    let source = "enum Flag {\n  case off, on\n  mutating func set(to value: Bool) -> Int {\n    if value { self = Flag.on } else { self = Flag.off }\n    return 7\n  }\n}\n\
         func main() {\n  var f = Flag.off\n  f.set(to: true)\n  print(f == Flag.on)\n}";
    assert_eq!(run(source), "true\n");
}

#[test]
fn a_mutating_enum_method_works_on_a_variable_in_a_nested_block() {
    let source = format!(
        "{LIGHT}func main() {{\n  var l = Light.red\n  var i = 0\n  while i < 3 {{\n    l.next()\n    i += 1\n  }}\n  print(l == Light.red)\n}}"
    );
    assert_eq!(run(&source), "true\n");
}

#[test]
fn a_mutating_enum_method_without_a_result_cannot_return_a_value() {
    let source =
        "enum Flag {\n  case off, on\n  mutating func f() { return 1 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("does not return a value"));
}

#[test]
fn a_mutating_enum_method_with_a_result_cannot_return_nothing() {
    let source =
        "enum Flag {\n  case off, on\n  mutating func f() -> Int { return }\n}\nfunc main() { }";
    assert!(build_error(source).contains("must return a value"));
}

#[test]
fn a_mutating_enum_method_cannot_be_called_on_a_constant() {
    let source = format!("{LIGHT}func main() {{\n  let l = Light.red\n  l.next()\n}}");
    assert!(build_error(&source).contains("mutating"));
}

#[test]
fn the_result_of_a_mutating_enum_method_cannot_be_used_in_an_expression() {
    let source = "enum Flag {\n  case off, on\n  mutating func flip() -> Int {\n    self = Flag.on\n    return 1\n  }\n}\n\
         func main() {\n  var f = Flag.off\n  let n = f.flip()\n  print(n)\n}";
    assert!(build_error(source).contains("statement"));
}

#[test]
fn calling_an_unknown_method_names_the_type_and_the_method() {
    let source = format!("{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  g.lower()\n}}");
    let message = build_error(&source);
    assert!(
        message.contains("Gauge") && message.contains("lower"),
        "{message}"
    );
}

#[test]
fn method_arguments_are_checked_like_function_arguments() {
    let label = format!("{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  g.raise(4)\n}}");
    assert!(build_error(&label).contains("label"));
    let count =
        format!("{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  g.raise(by: 1, by: 2)\n}}");
    assert!(build_error(&count).contains("argument"));
    let ty = format!("{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  g.raise(by: true)\n}}");
    assert!(build_error(&ty).contains("expected Int"));
}

#[test]
fn a_method_with_a_result_cannot_be_used_as_a_statement_value_of_the_wrong_type() {
    let source =
        format!("{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  let x: Bool = g.read()\n}}");
    assert!(build_error(&source).contains("expected Bool"));
}

#[test]
fn a_method_that_returns_nothing_cannot_be_used_as_a_value() {
    let source =
        format!("{GAUGE}func main() {{\n  let g = Gauge(level: 1)\n  let x = g.raise(by: 1)\n}}");
    assert!(build_error(&source).contains("does not produce a value"));
}

#[test]
fn a_method_cannot_be_declared_twice() {
    let source = "struct C { var n: Int\n  func f() { }\n  func f() { } }\nfunc main() { }";
    assert!(build_error(source).contains("duplicate"));
}

#[test]
fn a_method_may_declare_type_parameters_of_its_own() {
    let source = "struct C { var n: Int\n  func id<T>(_ x: T) -> T { return x } }\n\
         func main() {\n  let c = C(n: 1)\n  print(c.id(5))\n  print(c.id(true))\n  print(c.id(\"s\"))\n}";
    assert_eq!(run(source), "5\ntrue\ns\n");
}

#[test]
fn a_method_of_a_class_may_declare_type_parameters_of_its_own() {
    let source = "class C { var n: Int\n  func pick<T>(_ a: T, _ b: T, _ first: Bool) -> T { if first { return a } return b } }\n\
         func main() {\n  let c = C(n: 1)\n  print(c.pick(3, 4, false))\n}";
    assert_eq!(run(source), "4\n");
}

#[test]
fn a_generic_method_may_use_the_members_of_self() {
    let source = "struct C { var n: Int\n  func first<T>(_ x: T) -> Int { return n } }\n\
         func main() {\n  let c = C(n: 8)\n  print(c.first(true))\n}";
    assert_eq!(run(source), "8\n");
}

#[test]
fn a_method_type_parameter_cannot_repeat_a_type_parameter_of_its_owner() {
    let source = "struct Box<T> {\n  var value: T\n  func map<T>(_ x: T) -> T { return x }\n}\nfunc main() { }";
    assert!(build_error(source).contains("already"));
}

#[test]
fn a_method_type_parameter_that_no_argument_fixes_cannot_be_inferred() {
    let source = "struct C { var n: Int\n  func mk<T>() -> Int { return self.n } }\n\
         func main() {\n  let c = C(n: 1)\n  print(c.mk())\n}";
    assert!(build_error(source).contains("cannot infer"));
}

#[test]
fn an_initializer_cannot_declare_type_parameters() {
    let source = "struct C { var n: Int\n  init<T>(_ x: T) { self.n = 1 } }\nfunc main() { }";
    assert!(build_error(source).contains("initializer"));
}

#[test]
fn a_mutating_func_outside_a_type_is_rejected_at_the_top_level() {
    assert!(build_error("mutating func f() { }\nfunc main() { }").contains("top level"));
}

const ACCOUNT: &str = "class Account {\n  let owner: String\n  var balance: Int\n  init(owner: String, deposit: Int) {\n    self.owner = owner\n    self.balance = deposit * 2\n  }\n  func report() -> Int { return self.balance }\n}\n";

#[test]
fn a_class_initializer_computes_its_members_from_its_parameters() {
    let source = format!(
        "{ACCOUNT}func main() {{\n  let a = Account(owner: \"ann\", deposit: 5)\n  print(a.owner)\n  print(a.report())\n}}"
    );
    assert_eq!(run(&source), "ann\n10\n");
}

#[test]
fn a_struct_initializer_builds_a_value() {
    let source = "struct P {\n  var x: Int\n  var y: Int\n  init(side: Int) {\n    self.x = side\n    self.y = side\n  }\n  func area() -> Int { return self.x * self.y }\n}\n\
         func main() {\n  var p = P(side: 4)\n  p.x = 5\n  print(p.area())\n}";
    assert_eq!(run(source), "20\n");
}

#[test]
fn an_initializer_may_use_self_once_every_member_is_set() {
    let source = "class C {\n  var a: Int\n  var b: Int\n  init(a: Int) {\n    self.a = a\n    self.b = 0\n    self.b = self.a + 1\n  }\n}\n\
         func main() { print(C(a: 1).b) }";
    assert_eq!(run(source), "2\n");
}

#[test]
fn an_initializer_may_return_early_after_setting_every_member() {
    let source = "struct S {\n  var n: Int\n  init(n: Int) {\n    self.n = n\n    if n > 5 { return }\n    self.n = 0\n  }\n}\n\
         func main() {\n  print(S(n: 9).n)\n  print(S(n: 3).n)\n}";
    assert_eq!(run(source), "9\n0\n");
}

#[test]
fn an_initializer_may_run_other_statements_before_assigning() {
    let source = "struct S {\n  var n: Int\n  init(n: Int) {\n    var doubled = n\n    doubled += n\n    self.n = doubled\n  }\n}\n\
         func main() { print(S(n: 4).n) }";
    assert_eq!(run(source), "8\n");
}

#[test]
fn an_initializer_may_set_a_member_that_holds_a_class_instance() {
    let source = "class Inner { var v: Int }\nclass Outer {\n  var inner: Inner\n  init(v: Int) { self.inner = Inner(v: v) }\n}\n\
         func main() { print(Outer(v: 7).inner.v) }";
    assert_eq!(run(source), "7\n");
}

#[test]
fn a_custom_initializer_replaces_the_memberwise_one() {
    let source = "struct P {\n  var x: Int\n  init(side: Int) { self.x = side }\n}\nfunc main() { let p = P(x: 1) }";
    assert!(build_error(source).contains("label"));
}

#[test]
fn an_initializer_must_set_every_member() {
    let source =
        "struct P {\n  var x: Int\n  var y: Int\n  init() { self.x = 1 }\n}\nfunc main() { }";
    let message = build_error(source);
    assert!(
        message.contains("'y'") && message.contains("initialize"),
        "{message}"
    );
}

#[test]
fn assignments_inside_a_branch_do_not_count_as_initialization() {
    let source = "struct P {\n  var x: Int\n  init(flag: Bool) {\n    if flag { self.x = 1 } else { self.x = 2 }\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("initialize"));
}

#[test]
fn self_cannot_be_read_before_every_member_is_set() {
    let source = "struct P {\n  var x: Int\n  var y: Int\n  init() {\n    self.x = 1\n    self.y = self.x\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("before"));
}

#[test]
fn an_initializer_cannot_return_before_every_member_is_set() {
    let source = "struct P {\n  var x: Int\n  init(flag: Bool) {\n    if flag { return }\n    self.x = 1\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("before"));
}

#[test]
fn an_initializer_cannot_return_a_value() {
    let source = "struct P {\n  var x: Int\n  init() {\n    self.x = 1\n    return 1\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("initializer"));
}

#[test]
fn a_constant_member_is_assignable_in_an_initializer_but_not_in_a_method() {
    let source = "class C {\n  let n: Int\n  init() { self.n = 1 }\n  func bad() { self.n = 2 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("constant"));
}

#[test]
fn a_type_cannot_have_two_initializers() {
    let source = "struct P {\n  var x: Int\n  init() { self.x = 1 }\n  init() { self.x = 2 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("duplicate"));
}

const SWITCH: &str = "enum Switch {\n  case off, on\n  init(enabled: Bool) {\n    if enabled { self = Switch.on } else { self = Switch.off }\n  }\n}\n";

#[test]
fn an_initializer_of_an_enum_without_payloads_chooses_a_case() {
    let source = format!(
        "{SWITCH}func main() {{\n  print(Switch(enabled: true) == Switch.on)\n  print(Switch(enabled: false) == Switch.on)\n}}"
    );
    assert_eq!(run(&source), "true\nfalse\n");
}

#[test]
fn an_initializer_of_an_enum_with_payloads_builds_a_case() {
    let source = "enum Shape {\n  case empty\n  case circle(Int)\n  init(radius: Int) { self = Shape.circle(radius) }\n  func size() -> Int {\n    switch self {\n    case .empty: return 0\n    case .circle(let r): return r\n    }\n  }\n}\n\
         func main() {\n  print(Shape(radius: 7).size())\n}";
    assert_eq!(run(source), "7\n");
}

#[test]
fn an_initializer_of_a_generic_enum_infers_the_type_arguments() {
    let source = "enum Opt<T> {\n  case none\n  case some(T)\n  init(_ v: T) { self = Opt<T>.some(v) }\n  func orElse(_ d: T) -> T {\n    switch self {\n    case .none: return d\n    case .some(let v): return v\n    }\n  }\n}\n\
         func main() {\n  let o = Opt(5)\n  print(o.orElse(0))\n}";
    assert_eq!(run(source), "5\n");
}

#[test]
fn an_enum_initializer_may_use_self_once_it_is_set() {
    let source = "enum Light {\n  case red, green\n  init() {\n    self = Light.green\n    if self == Light.green { print(1) }\n  }\n}\n\
         func main() { let l = Light()\n print(l == Light.green) }";
    assert_eq!(run(source), "1\ntrue\n");
}

#[test]
fn an_enum_initializer_must_assign_self() {
    let source = "enum E {\n  case a\n  init() { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("self"));
}

#[test]
fn an_enum_initializer_cannot_use_self_before_assigning_it() {
    let source = "enum E {\n  case a, b\n  init() {\n    if self == E.a { print(1) }\n    self = E.b\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("self"));
}

#[test]
fn an_enum_initializer_cannot_assign_self_only_in_a_branch() {
    let source = "enum E {\n  case a, b\n  init(flag: Bool) {\n    if flag { self = E.a }\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("self"));
}

#[test]
fn init_outside_a_type_is_rejected_at_the_top_level() {
    assert!(build_error("init() { }\nfunc main() { }").contains("top level"));
}

const BOX: &str = "struct Box<T> {\n  var value: T\n  func get() -> T { return self.value }\n  mutating func set(_ v: T) { self.value = v }\n}\n";

#[test]
fn a_method_of_a_generic_struct_returns_the_type_argument() {
    let source = format!("{BOX}func main() {{\n  let b = Box(value: 5)\n  print(b.get())\n}}");
    assert_eq!(run(&source), "5\n");
}

#[test]
fn each_instantiation_of_a_generic_type_gets_its_own_method_instance() {
    let source = format!(
        "{BOX}func main() {{\n  let a = Box(value: 5)\n  let b = Box(value: true)\n  let c = Box(value: \"s\")\n  print(a.get())\n  print(b.get())\n  print(c.get())\n}}"
    );
    assert_eq!(run(&source), "5\ntrue\ns\n");
}

#[test]
fn a_mutating_method_of_a_generic_struct_writes_through_the_variable() {
    let source =
        format!("{BOX}func main() {{\n  var b = Box(value: 1)\n  b.set(9)\n  print(b.get())\n}}");
    assert_eq!(run(&source), "9\n");
}

#[test]
fn a_method_of_a_generic_class_changes_the_shared_instance() {
    let source = "class Cell<T> {\n  var value: T\n  func put(_ v: T) { self.value = v }\n  func take() -> T { return self.value }\n}\n\
         func main() {\n  let c = Cell(value: 1)\n  let d = c\n  d.put(7)\n  print(c.take())\n}";
    assert_eq!(run(source), "7\n");
}

#[test]
fn a_method_of_a_generic_type_can_call_another_method_through_self() {
    let source = "struct Box<T> {\n  var value: T\n  func get() -> T { return self.value }\n  func again() -> T { return self.get() }\n}\n\
         func main() { print(Box(value: 3).again()) }";
    assert_eq!(run(source), "3\n");
}

#[test]
fn a_generic_function_can_call_a_method_on_a_value_of_a_generic_type() {
    let source = format!(
        "{BOX}func read<T>(_ b: Box<T>) -> T {{ return b.get() }}\n\
         func main() {{\n  print(read(Box(value: 4)))\n  print(read(Box(value: false)))\n}}"
    );
    assert_eq!(run(&source), "4\nfalse\n");
}

#[test]
fn a_method_of_a_generic_enum_switches_on_self() {
    let source = "enum Opt<T> {\n  case none\n  case some(T)\n  func orElse(_ d: T) -> T {\n    switch self {\n    case .none: return d\n    case .some(let v): return v\n    }\n  }\n}\n\
         func main() {\n  let a = Opt<Int>.some(4)\n  let b = Opt<Int>.none\n  print(a.orElse(0))\n  print(b.orElse(8))\n}";
    assert_eq!(run(source), "4\n8\n");
}

#[test]
fn a_mutating_method_of_a_generic_enum_replaces_self() {
    let source = "enum Opt<T> {\n  case none\n  case some(T)\n  mutating func fill(_ v: T) { self = Opt<T>.some(v) }\n  func isSome() -> Bool {\n    switch self {\n    case .none: return false\n    case .some(_): return true\n    }\n  }\n}\n\
         func main() {\n  var o = Opt<Int>.none\n  o.fill(2)\n  print(o.isSome())\n}";
    assert_eq!(run(source), "true\n");
}

#[test]
fn an_initializer_of_a_generic_type_infers_the_type_arguments() {
    let source = "struct Pair<A, B> {\n  var first: A\n  var second: B\n  init(a: A, b: B) {\n    self.first = a\n    self.second = b\n  }\n  func swapped() -> Pair<B, A> { return Pair(a: self.second, b: self.first) }\n}\n\
         func main() {\n  let p = Pair(a: 1, b: true)\n  let q = p.swapped()\n  print(q.first)\n  print(q.second)\n}";
    assert_eq!(run(source), "true\n1\n");
}

#[test]
fn an_initializer_of_a_generic_class_may_take_explicit_type_arguments() {
    let source = "class Cell<T> {\n  var value: T\n  init(start: T) { self.value = start }\n}\n\
         func main() {\n  let c = Cell<Int>(start: 6)\n  print(c.value)\n}";
    assert_eq!(run(source), "6\n");
}

#[test]
fn explicit_type_arguments_are_checked_against_an_initializer() {
    let source = "class Cell<T> {\n  var value: T\n  init(start: T) { self.value = start }\n}\n\
         func main() { let c = Cell<Int>(start: true) }";
    assert!(build_error(source).contains("expected Int"));
}

#[test]
fn a_method_argument_of_the_wrong_type_is_rejected_for_a_generic_receiver() {
    let source = format!("{BOX}func main() {{\n  var b = Box(value: 1)\n  b.set(true)\n}}");
    assert!(build_error(&source).contains("expected Int"));
}

#[test]
fn a_method_body_of_a_generic_type_is_checked_even_when_unused() {
    let source = "struct Box<T> {\n  var value: T\n  func bad() -> Int { return self.value }\n}\nfunc main() { }";
    assert!(build_error(source).contains("expected Int"));
}

#[test]
fn a_method_of_a_generic_type_cannot_use_an_operator_on_the_type_parameter() {
    let source = "struct Box<T> {\n  var value: T\n  func twice() -> T { return self.value + self.value }\n}\nfunc main() { }";
    assert!(build_error(source).contains("expected Int"));
}

#[test]
fn a_generic_method_of_a_generic_type_infers_both_groups_of_type_arguments() {
    let source = String::from(
        "struct Pair<T> {\n  var a: T\n  func with<U>(_ b: U) -> U { return b }\n  func keep<U>(_ b: U) -> T { return self.a }\n}\n\
         func main() {\n  let p = Pair(a: 1)\n  print(p.with(true))\n  print(p.keep(\"s\"))\n}",
    );
    assert_eq!(run(&source), "true\n1\n");
}

#[test]
fn a_generic_method_of_a_generic_type_checks_its_body_once() {
    let source = "struct Box<T> {\n  var value: T\n  func map<U>(_ x: U) -> U { return x + x }\n}\nfunc main() { }";
    assert!(build_error(source).contains("expected Int"));
}

#[test]
fn a_method_reads_and_assigns_members_without_writing_self() {
    let source = "class Gauge {\n  var level: Int\n  func raise(by amount: Int) { level += amount }\n  func read() -> Int { return level * 2 }\n}\n\
         func main() {\n  let g = Gauge(level: 1)\n  g.raise(by: 4)\n  print(g.read())\n}";
    assert_eq!(run(source), "10\n");
}

#[test]
fn a_mutating_method_assigns_members_without_writing_self() {
    let source = "struct V {\n  var x: Int\n  var y: Int\n  mutating func shift(dx: Int) {\n    x += dx\n    y = y + dx\n  }\n}\n\
         func main() {\n  var v = V(x: 1, y: 2)\n  v.shift(dx: 10)\n  print(v.x)\n  print(v.y)\n}";
    assert_eq!(run(source), "11\n12\n");
}

#[test]
fn a_method_calls_another_method_without_writing_self() {
    let source = "struct C {\n  var n: Int\n  func read() -> Int { return n }\n  func double() -> Int { return read() * 2 }\n  mutating func inc() { n += 1 }\n  mutating func twice() {\n    inc()\n    inc()\n  }\n}\n\
         func main() {\n  var c = C(n: 20)\n  c.twice()\n  print(c.double())\n}";
    assert_eq!(run(source), "44\n");
}

#[test]
fn a_local_variable_shadows_a_member() {
    let source = "class C {\n  var n: Int\n  func f() -> Int {\n    let n = 100\n    return n\n  }\n  func g() -> Int { return n }\n}\n\
         func main() {\n  let c = C(n: 1)\n  print(c.f())\n  print(c.g())\n}";
    assert_eq!(run(source), "100\n1\n");
}

#[test]
fn a_parameter_shadows_a_member_and_self_reaches_the_member() {
    let source = "class C {\n  var n: Int\n  func set(n: Int) { self.n = n }\n}\n\
         func main() {\n  let c = C(n: 1)\n  c.set(n: 8)\n  print(c.n)\n}";
    assert_eq!(run(source), "8\n");
}

#[test]
fn a_method_of_a_generic_type_reads_its_members_without_writing_self() {
    let source = "struct Box<T> {\n  var value: T\n  func get() -> T { return value }\n  func again() -> T { return get() }\n}\n\
         func main() { print(Box(value: 5).again()) }";
    assert_eq!(run(source), "5\n");
}

#[test]
fn a_method_of_an_enum_calls_another_method_without_writing_self() {
    let source = "enum Light {\n  case red, green\n  func isRed() -> Bool {\n    switch self {\n    case .red: return true\n    case .green: return false\n    }\n  }\n  func isGreen() -> Bool { return !isRed() }\n}\n\
         func main() { print(Light.green.isGreen()) }";
    assert_eq!(run(source), "true\n");
}

#[test]
fn a_method_is_preferred_over_a_function_of_the_same_name() {
    let source = "func value() -> Int { return 1 }\nstruct C {\n  var n: Int\n  func value() -> Int { return n }\n  func f() -> Int { return value() }\n}\n\
         func main() {\n  print(C(n: 7).f())\n  print(value())\n}";
    assert_eq!(run(source), "7\n1\n");
}

#[test]
fn an_initializer_assigns_members_without_writing_self() {
    let source = "class C {\n  var a: Int\n  var b: Int\n  init(start: Int) {\n    a = start\n    b = start * 2\n  }\n}\n\
         func main() {\n  let c = C(start: 3)\n  print(c.a)\n  print(c.b)\n}";
    assert_eq!(run(source), "3\n6\n");
}

#[test]
fn an_initializer_may_use_members_and_methods_once_every_member_is_set() {
    let source = "class C {\n  var a: Int\n  var b: Int\n  func twice() -> Int { return a * 2 }\n  init(start: Int) {\n    a = start\n    b = 0\n    b = twice() + a\n  }\n}\n\
         func main() { print(C(start: 3).b) }";
    assert_eq!(run(source), "9\n");
}

#[test]
fn a_local_declared_in_an_initializer_shadows_a_member() {
    let source = "struct S {\n  var n: Int\n  init(start: Int) {\n    var m = start\n    m += 1\n    n = m\n  }\n}\n\
         func main() { print(S(start: 4).n) }";
    assert_eq!(run(source), "5\n");
}

#[test]
fn an_initializer_cannot_read_a_member_before_every_member_is_set() {
    let source = "struct P {\n  var x: Int\n  var y: Int\n  init() {\n    x = 1\n    y = x\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("before"));
}

#[test]
fn an_initializer_cannot_call_a_method_before_every_member_is_set() {
    let source = "struct P {\n  var x: Int\n  func f() -> Int { return 1 }\n  init() {\n    x = f()\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("before"));
}

#[test]
fn a_non_mutating_method_cannot_assign_a_member_implicitly() {
    let source = "struct C {\n  var n: Int\n  func bad() { n = 1 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("constant"));
}

#[test]
fn a_non_mutating_method_cannot_call_a_mutating_method_implicitly() {
    let source = "struct C {\n  var n: Int\n  mutating func inc() { n += 1 }\n  func bad() { inc() }\n}\nfunc main() { }";
    assert!(build_error(source).contains("mutating"));
}

#[test]
fn an_unknown_name_in_a_method_is_still_not_found() {
    let source =
        "struct C {\n  var n: Int\n  func f() -> Int { return missing }\n}\nfunc main() { }";
    assert!(build_error(source).contains("cannot find 'missing'"));
}

#[test]
fn a_member_name_is_not_visible_outside_the_type() {
    let source = "struct C { var n: Int }\nfunc f() -> Int { return n }\nfunc main() { }";
    assert!(build_error(source).contains("cannot find 'n'"));
}

#[test]
fn a_constant_member_cannot_be_assigned_implicitly_in_a_method() {
    let source =
        "class C {\n  let n: Int\n  init() { n = 1 }\n  func bad() { n = 2 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("constant"));
}

const NODE: &str = "class Node {\n  var v: Int\n}\n";

#[test]
fn identity_distinguishes_two_instances_with_equal_members() {
    let source = format!(
        "{NODE}func main() {{\n  let a = Node(v: 1)\n  let b = Node(v: 1)\n  print(a === a)\n  print(a === b)\n  print(a !== b)\n}}"
    );
    assert_eq!(run(&source), "true\nfalse\ntrue\n");
}

#[test]
fn identity_follows_a_reference_copied_into_another_variable() {
    let source = format!(
        "{NODE}func main() {{\n  let a = Node(v: 1)\n  let b = a\n  b.v = 5\n  print(a === b)\n  print(a.v)\n}}"
    );
    assert_eq!(run(&source), "true\n5\n");
}

#[test]
fn identity_is_rejected_for_values_that_are_not_class_instances() {
    let source = "func main() { print(1 === 1) }";
    assert!(build_error(source).contains("class"));
}

const SHAPES: &str =
    "class Shape {\n  var sides: Int\n}\nclass Square: Shape {\n  var length: Int\n}\n";

#[test]
fn a_subclass_inherits_the_members_of_its_superclass() {
    let source = format!(
        "{SHAPES}func main() {{\n  let s = Square(sides: 4, length: 3)\n  print(s.sides)\n  print(s.length)\n  s.sides = 5\n  print(s.sides)\n}}"
    );
    assert_eq!(run(&source), "4\n3\n5\n");
}

#[test]
fn a_subclass_instance_can_be_used_where_its_superclass_is_expected() {
    let source = format!(
        "{SHAPES}func sidesOf(_ s: Shape) -> Int {{ return s.sides }}\nfunc make() -> Shape {{ return Square(sides: 4, length: 1) }}\n\
         func main() {{\n  let a: Shape = Square(sides: 4, length: 3)\n  print(sidesOf(a))\n  print(sidesOf(Square(sides: 7, length: 1)))\n  print(make().sides)\n}}"
    );
    assert_eq!(run(&source), "4\n7\n4\n");
}

#[test]
fn a_superclass_instance_cannot_be_used_where_a_subclass_is_expected() {
    let source = format!("{SHAPES}func main() {{\n  let s: Square = Shape(sides: 4)\n}}");
    assert!(build_error(&source).contains("expected Square"));
}

#[test]
fn identity_compares_an_instance_with_its_superclass_reference() {
    let source = format!(
        "{SHAPES}func main() {{\n  let s = Square(sides: 4, length: 3)\n  let a: Shape = s\n  print(a === s)\n  print(s === a)\n  print(a !== Shape(sides: 4))\n}}"
    );
    assert_eq!(run(&source), "true\ntrue\ntrue\n");
}

#[test]
fn a_class_with_several_levels_of_inheritance_inherits_every_member() {
    let source = format!(
        "{SHAPES}class Cube: Square {{\n  var depth: Int\n}}\n\
         func sidesOf(_ s: Shape) -> Int {{ return s.sides }}\n\
         func main() {{\n  let c = Cube(sides: 4, length: 2, depth: 9)\n  print(c.depth)\n  print(c.length)\n  print(sidesOf(c))\n}}"
    );
    assert_eq!(run(&source), "9\n2\n4\n");
}

#[test]
fn unrelated_classes_are_not_interchangeable() {
    let source = format!(
        "{SHAPES}class Other {{ var n: Int }}\nfunc main() {{\n  let s: Shape = Other(n: 1)\n}}"
    );
    assert!(build_error(&source).contains("expected Shape"));
}

#[test]
fn a_superclass_must_exist() {
    let source = "class B: Missing { var y: Int }\nfunc main() { }";
    assert!(build_error(source).contains("Missing"));
}

#[test]
fn a_superclass_must_be_a_class() {
    let source = "struct S { var x: Int }\nclass B: S { var y: Int }\nfunc main() { }";
    assert!(build_error(source).contains("not a class"));
}

#[test]
fn a_class_cannot_inherit_from_itself_directly_or_not() {
    let direct = "class A: A { var x: Int }\nfunc main() { }";
    assert!(build_error(direct).contains("inherit"));
    let indirect = "class A: B { var x: Int }\nclass B: A { var y: Int }\nfunc main() { }";
    assert!(build_error(indirect).contains("inherit"));
}

#[test]
fn a_subclass_cannot_redeclare_an_inherited_member() {
    let source = "class A { var x: Int }\nclass B: A { var x: Int }\nfunc main() { }";
    assert!(build_error(source).contains("duplicate"));
}

#[test]
fn generic_classes_cannot_take_part_in_inheritance() {
    let sub = "class A { var x: Int }\nclass B<T>: A { var y: T }\nfunc main() { }";
    assert!(build_error(sub).contains("generic"));
    let sup = "class A<T> { var x: T }\nclass B: A<Int> { var y: Int }\nfunc main() { }";
    assert!(build_error(sup).contains("generic"));
}

const ANIMALS: &str = "class Animal {\n  var legs: Int\n  func sound() -> Int { return 1 }\n  func describe() -> Int { return self.sound() * 100 + legs }\n}\n\
class Dog: Animal {\n  override func sound() -> Int { return 2 }\n}\n\
class Puppy: Dog {\n  override func sound() -> Int { return super.sound() + 10 }\n}\n\
class Fish: Animal {\n  var fins: Int\n}\n";

#[test]
fn a_subclass_inherits_the_methods_of_its_superclass() {
    let source = format!(
        "{ANIMALS}func main() {{\n  let f = Fish(legs: 0, fins: 3)\n  print(f.sound())\n  print(f.describe())\n}}"
    );
    assert_eq!(run(&source), "1\n100\n");
}

#[test]
fn an_override_replaces_the_method_for_instances_of_the_subclass() {
    let source = format!("{ANIMALS}func main() {{\n  let d = Dog(legs: 4)\n  print(d.sound())\n}}");
    assert_eq!(run(&source), "2\n");
}

#[test]
fn a_call_through_the_superclass_type_runs_the_override_of_the_actual_class() {
    let source = format!(
        "{ANIMALS}func sound(of a: Animal) -> Int {{ return a.sound() }}\n\
         func main() {{\n  print(sound(of: Animal(legs: 4)))\n  print(sound(of: Dog(legs: 4)))\n  print(sound(of: Puppy(legs: 4)))\n  print(sound(of: Fish(legs: 0, fins: 1)))\n}}"
    );
    assert_eq!(run(&source), "1\n2\n12\n1\n");
}

#[test]
fn an_inherited_method_dispatches_on_self_to_the_override() {
    let source = format!(
        "{ANIMALS}func main() {{\n  print(Animal(legs: 4).describe())\n  print(Dog(legs: 4).describe())\n  print(Puppy(legs: 3).describe())\n}}"
    );
    assert_eq!(run(&source), "104\n204\n1203\n");
}

#[test]
fn super_calls_the_method_of_the_superclass_without_dispatching() {
    let source = format!("{ANIMALS}func main() {{\n  print(Puppy(legs: 4).sound())\n}}");
    assert_eq!(run(&source), "12\n");
}

#[test]
fn an_override_may_take_arguments_and_return_a_bool() {
    let source = "class A {\n  func fits(_ n: Int, limit: Int) -> Bool { return n < limit }\n}\n\
         class B: A {\n  override func fits(_ n: Int, limit: Int) -> Bool { return n <= limit }\n}\n\
         func check(_ a: A) -> Bool { return a.fits(5, limit: 5) }\n\
         func main() {\n  print(check(A()))\n  print(check(B()))\n}";
    assert_eq!(run(source), "false\ntrue\n");
}

#[test]
fn an_override_that_returns_nothing_runs_for_its_side_effects() {
    let source = "class Counter {\n  var n: Int\n  func bump() { self.n += 1 }\n}\n\
         class Double: Counter {\n  override func bump() { self.n += 2 }\n}\n\
         func twice(_ c: Counter) {\n  c.bump()\n  c.bump()\n}\n\
         func main() {\n  let a = Counter(n: 0)\n  let b = Double(n: 0)\n  twice(a)\n  twice(b)\n  print(a.n)\n  print(b.n)\n}";
    assert_eq!(run(source), "2\n4\n");
}

#[test]
fn an_override_without_an_overridden_method_is_rejected() {
    let source =
        "class A { var x: Int }\nclass B: A {\n  override func f() { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("override"));
}

#[test]
fn a_method_that_hides_an_inherited_one_must_say_override() {
    let source = "class A {\n  func f() { }\n}\nclass B: A {\n  func f() { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("override"));
}

#[test]
fn an_override_must_keep_the_signature() {
    let source = "class A {\n  func f() -> Int { return 1 }\n}\nclass B: A {\n  override func f() -> Bool { return true }\n}\nfunc main() { }";
    assert!(build_error(source).contains("signature"));
}

#[test]
fn override_is_rejected_outside_a_class() {
    let source = "struct S {\n  var x: Int\n  override func f() { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("class"));
}

#[test]
fn super_is_rejected_in_a_class_without_a_superclass() {
    let source = "class A {\n  func f() { super.f() }\n}\nfunc main() { }";
    assert!(build_error(source).contains("super"));
}

#[test]
fn super_must_name_a_method_of_the_superclass() {
    let source = "class A {\n  func f() { }\n}\nclass B: A {\n  override func f() { super.g() }\n}\nfunc main() { }";
    assert!(build_error(source).contains("g"));
}

#[test]
fn super_is_rejected_outside_a_method() {
    let source = "class A {\n  func f() { }\n}\nfunc g() { super.f() }\nfunc main() { }";
    assert!(build_error(source).contains("super"));
}

#[test]
fn an_override_may_return_a_struct() {
    let source = "struct P { var x: Int\n var y: Int }\n\
         class A {\n  func at() -> P { return P(x: 1, y: 2) }\n}\n\
         class B: A {\n  override func at() -> P { return P(x: 3, y: 4) }\n}\n\
         func sum(_ a: A) -> Int {\n  let p = a.at()\n  return p.x + p.y\n}\n\
         func main() {\n  print(sum(A()))\n  print(sum(B()))\n}";
    assert_eq!(run(source), "3\n7\n");
}

const VEHICLES: &str = "class Vehicle {\n  var wheels: Int\n  var name: Int\n  init(wheels: Int) {\n    self.wheels = wheels\n    self.name = wheels * 10\n  }\n  func kind() -> Int { return 1 }\n}\n\
class Car: Vehicle {\n  var doors: Int\n  init(doors: Int) {\n    self.doors = doors\n    super.init(wheels: 4)\n  }\n  override func kind() -> Int { return 2 }\n}\n";

#[test]
fn a_subclass_initializer_sets_its_members_then_calls_super_init() {
    let source = format!(
        "{VEHICLES}func main() {{\n  let c = Car(doors: 3)\n  print(c.doors)\n  print(c.wheels)\n  print(c.name)\n}}"
    );
    assert_eq!(run(&source), "3\n4\n40\n");
}

#[test]
fn an_instance_built_by_a_subclass_initializer_dispatches_as_the_subclass() {
    let source = format!(
        "{VEHICLES}func kind(of v: Vehicle) -> Int {{ return v.kind() }}\n\
         func main() {{\n  print(kind(of: Car(doors: 3)))\n  print(kind(of: Vehicle(wheels: 2)))\n}}"
    );
    assert_eq!(run(&source), "2\n1\n");
}

#[test]
fn super_init_may_use_the_memberwise_initializer_of_the_superclass() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  var y: Int\n  init(y: Int) {\n    self.y = y\n    super.init(x: y + 1)\n  }\n}\n\
         func main() {\n  let b = B(y: 1)\n  print(b.x)\n  print(b.y)\n}";
    assert_eq!(run(source), "2\n1\n");
}

#[test]
fn a_subclass_that_adds_no_members_may_start_with_super_init() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  init() { super.init(x: 9) }\n}\n\
         func main() {\n  print(B().x)\n}";
    assert_eq!(run(source), "9\n");
}

#[test]
fn a_subclass_initializer_may_use_self_after_super_init() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  var y: Int\n  init(y: Int) {\n    self.y = y\n    super.init(x: 1)\n    self.x += self.y\n    print(self.x)\n  }\n}\n\
         func main() {\n  let b = B(y: 5)\n  print(b.x)\n}";
    assert_eq!(run(source), "6\n6\n");
}

#[test]
fn a_subclass_initializer_must_call_super_init() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  var y: Int\n  init(y: Int) { self.y = y }\n}\nfunc main() { }";
    assert!(build_error(source).contains("super.init"));
}

#[test]
fn super_init_cannot_come_before_the_members_are_set() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  var y: Int\n  init(y: Int) {\n    super.init(x: 1)\n    self.y = y\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("super.init"));
}

#[test]
fn self_cannot_be_used_before_super_init() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  var y: Int\n  init(y: Int) {\n    self.y = y\n    print(self.y)\n    super.init(x: 1)\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("super.init"));
}

#[test]
fn an_inherited_member_cannot_be_set_before_super_init() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  var y: Int\n  init(y: Int) {\n    self.y = y\n    self.x = 1\n    super.init(x: 2)\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("self"));
}

#[test]
fn an_initializer_cannot_return_before_super_init() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  init() {\n    return\n    super.init(x: 1)\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("super.init"));
}

#[test]
fn super_init_cannot_be_called_twice() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  init() {\n    super.init(x: 1)\n    super.init(x: 2)\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("twice"));
}

#[test]
fn super_init_is_rejected_outside_an_initializer() {
    let source = "class A {\n  var x: Int\n}\nclass B: A {\n  func f() { super.init(x: 1) }\n}\nfunc main() { }";
    assert!(build_error(source).contains("initializer"));
}

#[test]
fn super_init_is_rejected_in_a_class_without_a_superclass() {
    let source = "class A {\n  var x: Int\n  init() {\n    self.x = 1\n    super.init()\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("super"));
}

#[test]
fn a_subclass_of_a_class_with_an_initializer_must_declare_its_own() {
    let source = format!(
        "{VEHICLES}class Truck: Vehicle {{\n  var load: Int\n}}\nfunc main() {{\n  let t = Truck(wheels: 6, name: 1, load: 2)\n}}"
    );
    assert!(build_error(&source).contains("initializer"));
}

#[test]
fn a_class_may_declare_a_deinitializer() {
    let source =
        "class Node {\n  var v: Int\n  deinit { print(self.v) }\n}\nfunc main() { print(1) }";
    assert_eq!(run(source), "1\n");
}

#[test]
fn a_deinitializer_may_use_members_without_writing_self() {
    let source = "class Node {\n  var v: Int\n  deinit { print(v) }\n}\nfunc main() { }";
    assert_eq!(run(source), "");
}

#[test]
fn a_struct_cannot_declare_a_deinitializer() {
    let source = "struct S {\n  var v: Int\n  deinit { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("class"));
}

#[test]
fn an_enum_cannot_declare_a_deinitializer() {
    let source = "enum E {\n  case a\n  deinit { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("class"));
}

#[test]
fn a_generic_class_cannot_declare_a_deinitializer_yet() {
    let source = "class Box<T> {\n  var v: T\n  deinit { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("generic"));
}

#[test]
fn a_class_cannot_declare_two_deinitializers() {
    let source = "class A {\n  var v: Int\n  deinit { }\n  deinit { }\n}\nfunc main() { }";
    assert!(build_error(source).contains("duplicate"));
}

#[test]
fn a_subclass_deinitializer_is_not_an_override() {
    let source =
        "class A {\n  var v: Int\n  deinit { }\n}\nclass B: A {\n  deinit { }\n}\nfunc main() { }";
    assert_eq!(run(source), "");
}

#[test]
fn deinit_outside_a_type_is_rejected_at_the_top_level() {
    assert!(build_error("deinit { }\nfunc main() { }").contains("top level"));
}

const TRACKED: &str = "class Tracked {\n  var id: Int\n  deinit { print(0 - self.id) }\n}\n";

fn tracked(body: &str) -> String {
    format!("{TRACKED}{body}")
}

#[test]
fn an_instance_is_deinitialized_when_its_last_variable_goes_out_of_scope() {
    let source = tracked("func main() {\n  let a = Tracked(id: 1)\n  print(10)\n}");
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn variables_are_released_in_reverse_order_of_declaration() {
    let source = tracked(
        "func main() {\n  let a = Tracked(id: 1)\n  let b = Tracked(id: 2)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-2\n-1\n");
}

#[test]
fn a_copied_reference_keeps_the_instance_alive_until_the_last_copy_goes() {
    let source = tracked(
        "func main() {\n  let a = Tracked(id: 1)\n  if true {\n    let b = a\n    print(10)\n  }\n  print(20)\n}",
    );
    assert_eq!(run(&source), "10\n20\n-1\n");
}

#[test]
fn assigning_a_variable_releases_the_instance_it_held() {
    let source = tracked(
        "func main() {\n  var a = Tracked(id: 1)\n  print(10)\n  a = Tracked(id: 2)\n  print(20)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n20\n-2\n");
}

#[test]
fn a_block_releases_its_variables_on_every_iteration() {
    let source = tracked(
        "func main() {\n  var i = 0\n  while i < 3 {\n    let t = Tracked(id: i + 1)\n    print(i)\n    i += 1\n  }\n}",
    );
    assert_eq!(run(&source), "0\n-1\n1\n-2\n2\n-3\n");
}

#[test]
fn a_parameter_does_not_own_its_argument() {
    let source = tracked(
        "func show(_ t: Tracked) { print(t.id) }\nfunc main() {\n  let a = Tracked(id: 1)\n  show(a)\n  show(a)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n1\n10\n-1\n");
}

#[test]
fn a_function_result_transfers_ownership_to_the_caller() {
    let source = tracked(
        "func make(_ id: Int) -> Tracked { return Tracked(id: id) }\nfunc main() {\n  let a = make(1)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_returned_local_survives_the_end_of_its_function() {
    let source = tracked(
        "func make() -> Tracked {\n  let t = Tracked(id: 1)\n  print(5)\n  return t\n}\nfunc main() {\n  let a = make()\n  print(10)\n}",
    );
    assert_eq!(run(&source), "5\n10\n-1\n");
}

#[test]
fn a_temporary_is_released_at_the_end_of_its_statement() {
    let source = tracked(
        "func make() -> Tracked { return Tracked(id: 1) }\nfunc main() {\n  print(make().id)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n10\n");
}

#[test]
fn a_discarded_result_is_released_immediately() {
    let source = tracked(
        "func make() -> Tracked { return Tracked(id: 1) }\nfunc main() {\n  make()\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n");
}

#[test]
fn an_early_return_releases_the_variables_declared_so_far() {
    let source = tracked(
        "func f(_ early: Bool) -> Int {\n  let a = Tracked(id: 1)\n  if early { return 5 }\n  let b = Tracked(id: 2)\n  return 6\n}\nfunc main() {\n  print(f(true))\n  print(f(false))\n}",
    );
    assert_eq!(run(&source), "-1\n5\n-2\n-1\n6\n");
}

#[test]
fn an_instance_releases_the_instances_its_members_refer_to() {
    let source = tracked(
        "class Holder {\n  var item: Tracked\n  deinit { print(100) }\n}\nfunc main() {\n  let h = Holder(item: Tracked(id: 1))\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n100\n-1\n");
}

#[test]
fn assigning_a_member_releases_the_instance_it_held() {
    let source = tracked(
        "class Holder {\n  var item: Tracked\n}\nfunc main() {\n  let h = Holder(item: Tracked(id: 1))\n  h.item = Tracked(id: 2)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn a_subclass_runs_its_deinitializer_before_its_superclass() {
    let source = "class A {\n  var x: Int\n  deinit { print(1) }\n}\nclass B: A {\n  deinit { print(2) }\n}\n\
         func main() {\n  let a: A = B(x: 0)\n  print(10)\n}";
    assert_eq!(run(source), "10\n2\n1\n");
}

#[test]
fn a_subclass_initializer_does_not_deinitialize_a_phantom_superclass_instance() {
    let source = "class A {\n  var x: Int\n  init(x: Int) { self.x = x }\n  deinit { print(1) }\n}\nclass B: A {\n  var y: Int\n  init(y: Int) {\n    self.y = y\n    super.init(x: 5)\n  }\n  deinit { print(2) }\n}\n\
         func main() {\n  let b = B(y: 1)\n  print(10)\n}";
    assert_eq!(run(source), "10\n2\n1\n");
}

#[test]
fn a_struct_that_holds_an_instance_retains_it_when_copied() {
    let source = tracked(
        "struct Pair {\n  var first: Tracked\n  var n: Int\n}\nfunc main() {\n  let p = Pair(first: Tracked(id: 1), n: 0)\n  if true {\n    let q = p\n    print(10)\n  }\n  print(20)\n}",
    );
    assert_eq!(run(&source), "10\n20\n-1\n");
}

#[test]
fn a_struct_returned_from_a_function_keeps_its_instance_alive() {
    let source = tracked(
        "struct Pair {\n  var first: Tracked\n  var n: Int\n}\nfunc make() -> Pair { return Pair(first: Tracked(id: 1), n: 0) }\nfunc main() {\n  let p = make()\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_mutating_method_releases_the_member_it_replaces() {
    let source = tracked(
        "struct Slot {\n  var item: Tracked\n  mutating func put(_ t: Tracked) { self.item = t }\n}\nfunc main() {\n  var s = Slot(item: Tracked(id: 1))\n  s.put(Tracked(id: 2))\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn a_generic_container_holds_an_instance_until_it_is_released() {
    let source = tracked(
        "class Box<T> {\n  var value: T\n}\nfunc main() {\n  let b = Box(value: Tracked(id: 1))\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_method_called_on_a_temporary_instance_keeps_it_alive_for_the_call() {
    let source = tracked(
        "class Counter {\n  var n: Int\n  func read() -> Int { return self.n }\n  deinit { print(99) }\n}\nfunc main() {\n  print(Counter(n: 4).read())\n  print(10)\n}",
    );
    assert_eq!(run(&source), "4\n99\n10\n");
}

#[test]
fn a_condition_that_creates_an_instance_releases_it_before_the_branch() {
    let source = tracked(
        "func make() -> Tracked { return Tracked(id: 1) }\nfunc main() {\n  if make().id == 1 {\n    print(10)\n  }\n}",
    );
    assert_eq!(run(&source), "-1\n10\n");
}

#[test]
fn a_loop_condition_releases_its_temporaries_on_every_test() {
    let source = tracked(
        "func make(_ n: Int) -> Tracked { return Tracked(id: n) }\nfunc main() {\n  var i = 1\n  while make(i).id < 3 {\n    i += 1\n  }\n}",
    );
    assert_eq!(run(&source), "-1\n-2\n-3\n");
}

#[test]
fn many_instances_can_be_created_and_released() {
    let source = "class Node {\n  var v: Int\n}\nfunc main() {\n  var i = 0\n  var total = 0\n  while i < 100000 {\n    let n = Node(v: i)\n    total += n.v - i + 1\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "100000\n");
}

#[test]
fn assigning_a_variable_to_itself_keeps_its_instance() {
    let source = tracked("func main() {\n  var a = Tracked(id: 1)\n  a = a\n  print(a.id)\n}");
    assert_eq!(run(&source), "1\n-1\n");
}

#[test]
fn a_method_can_return_the_instance_held_by_a_member() {
    let source = tracked(
        "class Holder {\n  var item: Tracked\n  func get() -> Tracked { return self.item }\n}\nfunc main() {\n  let h = Holder(item: Tracked(id: 1))\n  let t = h.get()\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_member_can_be_assigned_from_another_member() {
    let source = tracked(
        "class Pair {\n  var a: Tracked\n  var b: Tracked\n}\nfunc main() {\n  let p = Pair(a: Tracked(id: 1), b: Tracked(id: 2))\n  p.a = p.b\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn a_recursive_function_passes_instances_through_its_frames() {
    let source = tracked(
        "func build(_ n: Int) -> Tracked {\n  if n == 0 { return Tracked(id: 100) }\n  let inner = build(n - 1)\n  return inner\n}\nfunc main() {\n  let t = build(3)\n  print(t.id)\n}",
    );
    assert_eq!(run(&source), "100\n-100\n");
}

#[test]
fn an_instance_passed_to_an_initializer_is_owned_by_the_new_instance() {
    let source = tracked(
        "class Holder {\n  var item: Tracked\n  init(item: Tracked) { self.item = item }\n}\nfunc main() {\n  let t = Tracked(id: 1)\n  let h = Holder(item: t)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_virtual_call_keeps_a_temporary_argument_alive() {
    let source = tracked(
        "class Base {\n  func use(_ t: Tracked) -> Int { return t.id }\n}\nclass Derived: Base {\n  override func use(_ t: Tracked) -> Int { return t.id + 1 }\n}\nfunc main() {\n  let b: Base = Derived()\n  print(b.use(Tracked(id: 5)))\n  print(10)\n}",
    );
    assert_eq!(run(&source), "6\n-5\n10\n");
}

#[test]
fn an_instance_created_in_a_for_loop_is_released_on_every_iteration() {
    let source = tracked(
        "func main() {\n  for i in 1...3 {\n    let t = Tracked(id: i)\n    print(i * 10)\n  }\n}",
    );
    assert_eq!(run(&source), "10\n-1\n20\n-2\n30\n-3\n");
}

#[test]
fn a_for_loop_bound_that_creates_an_instance_releases_it() {
    let source = tracked(
        "func make() -> Tracked { return Tracked(id: 3) }\nfunc main() {\n  for i in 1...make().id {\n    print(i)\n  }\n}",
    );
    assert_eq!(run(&source), "-3\n1\n2\n3\n");
}

#[test]
fn a_printed_expression_that_creates_an_instance_releases_it() {
    let source = tracked("func main() {\n  print(Tracked(id: 7).id)\n}");
    assert_eq!(run(&source), "7\n-7\n");
}

#[test]
fn a_short_circuit_that_skips_a_temporary_releases_nothing_twice() {
    let source = tracked(
        "func make() -> Tracked { return Tracked(id: 1) }\nfunc main() {\n  var i = 0\n  while i < 3 {\n    if i == 0 && make().id == 1 { print(10) }\n    if i == 5 && make().id == 1 { print(20) }\n    i += 1\n  }\n}",
    );
    assert_eq!(run(&source), "-1\n10\n");
}

#[test]
fn a_temporary_struct_holding_an_instance_is_released_after_the_call() {
    let source = tracked(
        "struct Pair {\n  var first: Tracked\n  var n: Int\n}\nfunc make() -> Pair { return Pair(first: Tracked(id: 1), n: 5) }\nfunc size(_ p: Pair) -> Int { return p.n }\nfunc main() {\n  print(size(make()))\n  print(10)\n}",
    );
    assert_eq!(run(&source), "5\n-1\n10\n");
}

#[test]
fn assigning_a_struct_variable_releases_the_instance_it_held() {
    let source = tracked(
        "struct Pair {\n  var first: Tracked\n  var n: Int\n}\nfunc main() {\n  var p = Pair(first: Tracked(id: 1), n: 0)\n  p = Pair(first: Tracked(id: 2), n: 0)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

const FAMILY: &str = "class Parent {\n  var id: Int\n  deinit { print(0 - self.id) }\n}\nclass Child {\n  unowned var parent: Parent\n  var id: Int\n  deinit { print(0 - self.id) }\n}\n";

fn family(body: &str) -> String {
    format!("{FAMILY}{body}")
}

#[test]
fn an_unowned_member_reads_the_instance_it_refers_to() {
    let source = family(
        "func main() {\n  let p = Parent(id: 1)\n  let c = Child(parent: p, id: 2)\n  print(c.parent.id)\n}",
    );
    assert_eq!(run(&source), "1\n-2\n-1\n");
}

#[test]
fn an_unowned_member_does_not_keep_its_instance_alive() {
    let source =
        family("func main() {\n  let c = Child(parent: Parent(id: 1), id: 2)\n  print(10)\n}");
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn reading_an_unowned_member_after_its_instance_died_traps() {
    let source = family(
        "func make() -> Child {\n  let p = Parent(id: 1)\n  return Child(parent: p, id: 2)\n}\nfunc main() {\n  let c = make()\n  print(c.parent.id)\n}",
    );
    run_expecting_a_crash(&source);
}

#[test]
fn an_unowned_reference_to_a_dead_instance_can_be_released_without_reading_it() {
    let source = family(
        "func make() -> Child {\n  let p = Parent(id: 1)\n  return Child(parent: p, id: 2)\n}\nfunc main() {\n  let c = make()\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn assigning_an_unowned_member_switches_the_instance_it_refers_to() {
    let source = family(
        "func main() {\n  let a = Parent(id: 1)\n  let b = Parent(id: 3)\n  let c = Child(parent: a, id: 2)\n  c.parent = b\n  print(c.parent.id)\n}",
    );
    assert_eq!(run(&source), "3\n-2\n-3\n-1\n");
}

#[test]
fn a_variable_initialized_from_an_unowned_member_keeps_the_instance_alive() {
    let source = family(
        "func make() -> Child {\n  let p = Parent(id: 1)\n  return Child(parent: p, id: 2)\n}\nfunc main() {\n  let p = Parent(id: 5)\n  let c = Child(parent: p, id: 6)\n  let q = c.parent\n  print(q.id)\n}",
    );
    assert_eq!(run(&source), "5\n-6\n-5\n");
}

#[test]
fn an_unowned_member_can_be_passed_to_a_function() {
    let source = family(
        "func read(_ p: Parent) -> Int { return p.id }\nfunc main() {\n  let p = Parent(id: 4)\n  let c = Child(parent: p, id: 6)\n  print(read(c.parent))\n}",
    );
    assert_eq!(run(&source), "4\n-6\n-4\n");
}

#[test]
fn an_unowned_member_is_inherited_by_subclasses() {
    let source = "class Parent {\n  var id: Int\n}\nclass Child {\n  unowned let parent: Parent\n}\nclass Grandchild: Child {\n  var n: Int\n}\n\
         func main() {\n  let p = Parent(id: 7)\n  let g = Grandchild(parent: p, n: 1)\n  print(g.parent.id)\n}";
    assert_eq!(run(source), "7\n");
}

#[test]
fn an_unowned_member_is_released_by_an_initializer_that_sets_it() {
    let source = "class Parent {\n  var id: Int\n}\nclass Child {\n  unowned var parent: Parent\n  init(of p: Parent) { self.parent = p }\n}\n\
         func main() {\n  let p = Parent(id: 8)\n  let c = Child(of: p)\n  print(c.parent.id)\n}";
    assert_eq!(run(source), "8\n");
}

#[test]
fn unowned_is_rejected_on_a_struct_member() {
    let source = "class P { var x: Int }\nstruct S {\n  unowned var p: P\n}\nfunc main() { }";
    assert!(build_error(source).contains("unowned"));
}

#[test]
fn unowned_is_rejected_on_a_member_that_is_not_a_class() {
    let source = "class C {\n  unowned var n: Int\n}\nfunc main() { }";
    assert!(build_error(source).contains("unowned"));
}

const REGION: &str = "struct Point {\n  var x: Int\n  var y: Int\n}\nstruct Region {\n  var origin: Point\n  var size: Int\n  var visible: Bool\n}\n";

#[test]
fn a_struct_can_hold_another_struct() {
    let source = format!(
        "{REGION}func main() {{\n  let r = Region(origin: Point(x: 1, y: 2), size: 3, visible: true)\n  print(r.origin.x)\n  print(r.origin.y)\n  print(r.size)\n  print(r.visible)\n}}"
    );
    assert_eq!(run(&source), "1\n2\n3\ntrue\n");
}

#[test]
fn a_nested_struct_is_copied_by_value() {
    let source = format!(
        "{REGION}func main() {{\n  var r = Region(origin: Point(x: 1, y: 2), size: 3, visible: true)\n  let copy = r\n  r.origin = Point(x: 9, y: 8)\n  print(copy.origin.x)\n  print(r.origin.x)\n  let p = r.origin\n  r.origin = Point(x: 0, y: 0)\n  print(p.y)\n}}"
    );
    assert_eq!(run(&source), "1\n9\n8\n");
}

#[test]
fn a_struct_holding_a_struct_can_be_passed_and_returned() {
    let source = format!(
        "{REGION}func grow(_ r: Region) -> Region {{\n  return Region(origin: r.origin, size: r.size + 1, visible: r.visible)\n}}\n\
         func main() {{\n  let r = grow(Region(origin: Point(x: 4, y: 5), size: 1, visible: false))\n  print(r.origin.y)\n  print(r.size)\n  print(r.visible)\n}}"
    );
    assert_eq!(run(&source), "5\n2\nfalse\n");
}

#[test]
fn a_member_of_a_nested_struct_stays_in_place_when_a_neighbour_is_assigned() {
    let source = format!(
        "{REGION}func main() {{\n  var r = Region(origin: Point(x: 1, y: 2), size: 3, visible: true)\n  r.size = 7\n  r.visible = false\n  print(r.origin.x)\n  print(r.origin.y)\n  print(r.size)\n  print(r.visible)\n}}"
    );
    assert_eq!(run(&source), "1\n2\n7\nfalse\n");
}

#[test]
fn a_mutating_method_can_replace_a_nested_struct() {
    let source = "struct Point {\n  var x: Int\n}\nstruct Box {\n  var corner: Point\n  var n: Int\n  mutating func move(to p: Point) { self.corner = p }\n  func cornerX() -> Int { return corner.x }\n}\n\
         func main() {\n  var b = Box(corner: Point(x: 1), n: 0)\n  b.move(to: Point(x: 6))\n  print(b.cornerX())\n}";
    assert_eq!(run(source), "6\n");
}

#[test]
fn a_class_can_hold_a_struct_member() {
    let source = "struct Point {\n  var x: Int\n  var y: Int\n}\nclass Sprite {\n  var at: Point\n  var id: Int\n  func shift() { self.at = Point(x: self.at.x + 1, y: self.at.y) }\n}\n\
         func main() {\n  let s = Sprite(at: Point(x: 1, y: 2), id: 5)\n  s.shift()\n  print(s.at.x)\n  print(s.at.y)\n  print(s.id)\n}";
    assert_eq!(run(source), "2\n2\n5\n");
}

#[test]
fn an_initializer_can_set_a_struct_member() {
    let source = "struct Point {\n  var x: Int\n}\nstruct Wrap {\n  var p: Point\n  var n: Int\n  init(x: Int) {\n    self.p = Point(x: x)\n    self.n = x + 1\n  }\n}\n\
         func main() {\n  let w = Wrap(x: 4)\n  print(w.p.x)\n  print(w.n)\n}";
    assert_eq!(run(source), "4\n5\n");
}

#[test]
fn an_enum_with_payloads_can_be_a_struct_member() {
    let source = "enum Shape {\n  case empty\n  case circle(Int)\n}\nstruct Slot {\n  var shape: Shape\n  var n: Int\n}\n\
         func main() {\n  let s = Slot(shape: Shape.circle(7), n: 2)\n  switch s.shape {\n  case .empty: print(0)\n  case .circle(let r): print(r)\n  }\n  print(s.n)\n}";
    assert_eq!(run(source), "7\n2\n");
}

#[test]
fn a_struct_cannot_contain_itself() {
    let source = "struct Loop {\n  var inner: Loop\n}\nfunc main() { }";
    assert!(build_error(source).contains("contain itself"));
}

#[test]
fn a_struct_nested_two_levels_deep_keeps_every_member() {
    let source = "struct A { var a: Int }\nstruct B { var x: Int\n var a: A }\nstruct C { var y: Int\n var b: B\n var z: Int }\n\
         func main() {\n  let c = C(y: 1, b: B(x: 2, a: A(a: 3)), z: 4)\n  let d = c\n  print(d.y)\n  print(d.b.x)\n  print(d.b.a.a)\n  print(d.z)\n}";
    assert_eq!(run(source), "1\n2\n3\n4\n");
}

#[test]
fn a_nested_struct_that_holds_an_instance_retains_it_on_copy() {
    let source = tracked(
        "struct Inner {\n  var item: Tracked\n}\nstruct Outer {\n  var inner: Inner\n  var n: Int\n}\nfunc main() {\n  let o = Outer(inner: Inner(item: Tracked(id: 1)), n: 0)\n  if true {\n    let q = o\n    print(10)\n  }\n  print(20)\n}",
    );
    assert_eq!(run(&source), "10\n20\n-1\n");
}

#[test]
fn replacing_a_nested_struct_releases_the_instance_it_held() {
    let source = tracked(
        "struct Inner {\n  var item: Tracked\n}\nclass Holder {\n  var inner: Inner\n}\nfunc main() {\n  let h = Holder(inner: Inner(item: Tracked(id: 1)))\n  h.inner = Inner(item: Tracked(id: 2))\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn an_optional_holds_a_value_or_nil() {
    let source = "func main() {\n  let a: Int? = 5\n  let b: Int? = nil\n  print(a == nil)\n  print(b == nil)\n  print(a != nil)\n  print(b != nil)\n  print(a!)\n}";
    assert_eq!(run(source), "false\ntrue\ntrue\nfalse\n5\n");
}

#[test]
fn a_value_is_promoted_to_an_optional_where_one_is_expected() {
    let source = "func half(_ n: Int) -> Int? {\n  if n % 2 == 0 { return n / 2 }\n  return nil\n}\nfunc show(_ x: Int?) -> Int {\n  if x == nil { return 0 - 1 }\n  return x!\n}\nfunc main() {\n  print(show(half(8)))\n  print(show(half(7)))\n  print(show(3))\n  print(show(nil))\n}";
    assert_eq!(run(source), "4\n-1\n3\n-1\n");
}

#[test]
fn an_optional_can_be_assigned_a_value_or_nil_later() {
    let source = "func main() {\n  var x: Int? = nil\n  print(x == nil)\n  x = 7\n  print(x!)\n  x = nil\n  print(x == nil)\n}";
    assert_eq!(run(source), "true\n7\ntrue\n");
}

#[test]
fn an_optional_can_hold_a_bool_a_string_or_an_enum() {
    let source = "enum Color { case red, green }\nfunc main() {\n  let b: Bool? = true\n  let s: String? = \"hi\"\n  let c: Color? = Color.green\n  print(b!)\n  print(s!)\n  print(c! == Color.green)\n}";
    assert_eq!(run(source), "true\nhi\ntrue\n");
}

#[test]
fn an_optional_can_be_a_member_of_a_struct_and_a_class() {
    let source = "struct S {\n  var v: Int?\n  var n: Int\n}\nclass C {\n  var v: Int?\n}\nfunc main() {\n  let s = S(v: 3, n: 1)\n  let t = S(v: nil, n: 2)\n  let c = C(v: nil)\n  print(s.v!)\n  print(t.v == nil)\n  c.v = 9\n  print(c.v!)\n  print(s.n + t.n)\n}";
    assert_eq!(run(source), "3\ntrue\n9\n3\n");
}

#[test]
fn an_optional_of_a_struct_keeps_the_whole_struct() {
    let source = "struct P { var x: Int\n var y: Int }\nfunc make(_ ok: Bool) -> P? {\n  if ok { return P(x: 1, y: 2) }\n  return nil\n}\nfunc main() {\n  let a = make(true)\n  let b = make(false)\n  print(a!.x)\n  print(a!.y)\n  print(b == nil)\n}";
    assert_eq!(run(source), "1\n2\ntrue\n");
}

#[test]
fn an_optional_of_an_optional_wraps_twice() {
    let source = "func main() {\n  let a: Int?? = 4\n  let b: Int?? = nil\n  print(a == nil)\n  print(b == nil)\n  print(a!!)\n}";
    assert_eq!(run(source), "false\ntrue\n4\n");
}

#[test]
fn an_optional_instance_is_retained_and_released_like_an_instance() {
    let source = tracked(
        "func main() {\n  var t: Tracked? = Tracked(id: 1)\n  print(10)\n  t = nil\n  print(20)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n20\n");
}

#[test]
fn an_optional_instance_goes_out_of_scope_with_its_variable() {
    let source = tracked(
        "func main() {\n  let t: Tracked? = Tracked(id: 1)\n  let u: Tracked? = t\n  print(10)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n");
}

#[test]
fn a_class_can_refer_to_itself_through_an_optional() {
    let source = "class Node {\n  var value: Int\n  var next: Node?\n}\nfunc main() {\n  let c = Node(value: 3, next: nil)\n  let b = Node(value: 2, next: c)\n  let a = Node(value: 1, next: b)\n  print(a.next!.value)\n  print(a.next!.next!.value)\n  print(c.next == nil)\n}";
    assert_eq!(run(source), "2\n3\ntrue\n");
}

#[test]
fn a_linked_list_releases_every_node() {
    let source = "class Node {\n  var value: Int\n  var next: Node?\n  deinit { print(0 - self.value) }\n}\nfunc main() {\n  let c = Node(value: 3, next: nil)\n  let b = Node(value: 2, next: c)\n  let a = Node(value: 1, next: b)\n  print(10)\n}";
    assert_eq!(run(source), "10\n-1\n-2\n-3\n");
}

#[test]
fn unwrapping_nil_traps() {
    run_expecting_a_crash("func main() {\n  let x: Int? = nil\n  print(x!)\n}");
}

#[test]
fn nil_needs_a_type_from_its_context() {
    assert!(build_error("func main() {\n  let x = nil\n}").contains("nil"));
}

#[test]
fn comparing_a_value_that_is_not_an_optional_with_nil_is_rejected() {
    assert!(build_error("func main() {\n  let x = 1\n  print(x == nil)\n}").contains("optional"));
}

#[test]
fn unwrapping_a_value_that_is_not_an_optional_is_rejected() {
    assert!(build_error("func main() {\n  let x = 1\n  print(x!)\n}").contains("optional"));
}

#[test]
fn an_optional_is_not_accepted_where_its_wrapped_type_is_expected() {
    let source = "func f(_ n: Int) { }\nfunc main() {\n  let x: Int? = 1\n  f(x)\n}";
    assert!(build_error(source).contains("expected Int"));
}

#[test]
fn an_optional_type_prints_with_a_question_mark_in_errors() {
    let source = "func f(_ n: Int?) { }\nfunc main() {\n  f(true)\n}";
    assert!(build_error(source).contains("Int?"));
}

#[test]
fn a_subclass_instance_is_promoted_to_an_optional_superclass() {
    let source = "class A { var x: Int }\nclass B: A { var y: Int }\nfunc main() {\n  let a: A? = B(x: 1, y: 2)\n  print(a!.x)\n}";
    assert_eq!(run(source), "1\n");
}

#[test]
fn nil_coalescing_takes_the_value_or_the_default() {
    let source = "func main() {\n  let a: Int? = 4\n  let b: Int? = nil\n  print(a ?? 0)\n  print(b ?? 9)\n  print((b ?? 1) + (a ?? 1))\n}";
    assert_eq!(run(source), "4\n9\n5\n");
}

#[test]
fn nil_coalescing_does_not_evaluate_the_default_when_there_is_a_value() {
    let source = "func noisy() -> Int {\n  print(100)\n  return 1\n}\nfunc main() {\n  let a: Int? = 4\n  print(a ?? noisy())\n  let b: Int? = nil\n  print(b ?? noisy())\n}";
    assert_eq!(run(source), "4\n100\n1\n");
}

#[test]
fn nil_coalescing_may_yield_an_optional() {
    let source = "func main() {\n  let a: Int? = nil\n  let b: Int? = 6\n  let c: Int? = a ?? b\n  let d: Int? = a ?? nil\n  print(c!)\n  print(d == nil)\n}";
    assert_eq!(run(source), "6\ntrue\n");
}

#[test]
fn nil_coalescing_chains_to_the_right() {
    let source = "func main() {\n  let a: Int? = nil\n  let b: Int? = nil\n  print(a ?? b ?? 3)\n}";
    assert_eq!(run(source), "3\n");
}

#[test]
fn nil_coalescing_binds_tighter_than_comparison() {
    let source = "func main() {\n  let a: Int? = nil\n  print(a ?? 5 == 5)\n}";
    assert_eq!(run(source), "true\n");
}

#[test]
fn nil_coalescing_retains_an_instance_it_returns() {
    let source = tracked(
        "func main() {\n  let a: Tracked? = Tracked(id: 1)\n  let t = a ?? Tracked(id: 2)\n  print(t.id)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n");
}

#[test]
fn nil_coalescing_releases_the_default_it_did_not_use() {
    let source = tracked(
        "func main() {\n  let a: Tracked? = nil\n  let t = a ?? Tracked(id: 2)\n  print(t.id)\n}",
    );
    assert_eq!(run(&source), "2\n-2\n");
}

#[test]
fn nil_coalescing_needs_an_optional_on_the_left() {
    assert!(build_error("func main() {\n  let a = 1\n  print(a ?? 2)\n}").contains("optional"));
}

#[test]
fn if_let_binds_the_value_when_there_is_one() {
    let source = "func show(_ x: Int?) {\n  if let v = x {\n    print(v)\n  } else {\n    print(0 - 1)\n  }\n}\nfunc main() {\n  show(5)\n  show(nil)\n}";
    assert_eq!(run(source), "5\n-1\n");
}

#[test]
fn if_let_without_else_skips_the_body_for_nil() {
    let source = "func main() {\n  let x: Int? = nil\n  if let v = x { print(v) }\n  print(1)\n}";
    assert_eq!(run(source), "1\n");
}

#[test]
fn if_let_chains_through_else_if() {
    let source = "func pick(_ a: Int?, _ b: Int?) -> Int {\n  if let x = a {\n    return x\n  } else if let y = b {\n    return y + 100\n  } else {\n    return 0\n  }\n}\nfunc main() {\n  print(pick(1, 2))\n  print(pick(nil, 2))\n  print(pick(nil, nil))\n}";
    assert_eq!(run(source), "1\n102\n0\n");
}

#[test]
fn if_let_binding_is_scoped_to_its_body() {
    let source =
        "func main() {\n  let x: Int? = 3\n  if let v = x { print(v) }\n  let v = 8\n  print(v)\n}";
    assert_eq!(run(source), "3\n8\n");
}

#[test]
fn if_let_binds_an_instance_for_the_length_of_the_body() {
    let source = tracked(
        "func main() {\n  var a: Tracked? = Tracked(id: 1)\n  if let t = a {\n    a = nil\n    print(t.id)\n  }\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n10\n");
}

#[test]
fn if_let_evaluates_a_temporary_once() {
    let source = tracked(
        "func make() -> Tracked? { return Tracked(id: 1) }\nfunc main() {\n  if let t = make() {\n    print(t.id)\n  }\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n10\n");
}

#[test]
fn if_let_needs_an_optional() {
    assert!(
        build_error("func main() {\n  let a = 1\n  if let v = a { print(v) }\n}")
            .contains("optional")
    );
}

#[test]
fn optional_chaining_reads_a_member_through_an_optional() {
    let source = "class Node {\n  var value: Int\n  var next: Node?\n}\nfunc main() {\n  let b = Node(value: 2, next: nil)\n  let a = Node(value: 1, next: b)\n  print(a.next?.value ?? 0)\n  print(b.next?.value ?? 0)\n  let c: Node? = a\n  print(c?.value ?? 0)\n}";
    assert_eq!(run(source), "2\n0\n1\n");
}

#[test]
fn optional_chaining_flattens_an_optional_member() {
    let source = "class Node {\n  var value: Int\n  var next: Node?\n}\nfunc main() {\n  let c = Node(value: 3, next: nil)\n  let b = Node(value: 2, next: c)\n  let a = Node(value: 1, next: b)\n  print(a.next?.next?.value ?? 0)\n  print(c.next?.next?.value ?? 0)\n  print(a.next?.next == nil)\n}";
    assert_eq!(run(source), "3\n0\nfalse\n");
}

#[test]
fn optional_chaining_calls_a_method() {
    let source = "class Counter {\n  var n: Int\n  func doubled() -> Int { return self.n * 2 }\n  func bump() { self.n += 1 }\n}\nfunc main() {\n  let a: Counter? = Counter(n: 4)\n  let b: Counter? = nil\n  print(a?.doubled() ?? 0)\n  print(b?.doubled() ?? 0)\n  a?.bump()\n  b?.bump()\n  print(a!.n)\n}";
    assert_eq!(run(source), "8\n0\n5\n");
}

#[test]
fn optional_chaining_skips_the_call_for_nil() {
    let source = "func noisy() -> Int {\n  print(100)\n  return 1\n}\nclass C {\n  var n: Int\n  func add(_ k: Int) -> Int { return self.n + k }\n}\nfunc main() {\n  let b: C? = nil\n  print(b?.add(noisy()) ?? 0)\n}";
    assert_eq!(run(source), "0\n");
}

#[test]
fn optional_chaining_releases_the_instances_it_touches() {
    let source = tracked(
        "class Holder {\n  var item: Tracked?\n}\nfunc make() -> Holder? { return Holder(item: Tracked(id: 1)) }\nfunc main() {\n  print(make()?.item?.id ?? 0)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n10\n");
}

#[test]
fn optional_chaining_needs_an_optional() {
    let source = "class C { var n: Int }\nfunc main() {\n  let c = C(n: 1)\n  print(c?.n ?? 0)\n}";
    assert!(build_error(source).contains("optional"));
}

const WEAK_FAMILY: &str = "class Parent {\n  var id: Int\n  var child: Child?\n  deinit { print(0 - self.id) }\n}\nclass Child {\n  weak var parent: Parent?\n  var id: Int\n  deinit { print(0 - self.id) }\n}\n";

fn weak_family(body: &str) -> String {
    format!("{WEAK_FAMILY}{body}")
}

#[test]
fn a_weak_member_reads_the_instance_while_it_is_alive() {
    let source = weak_family(
        "func main() {\n  let p = Parent(id: 1, child: nil)\n  let c = Child(parent: p, id: 2)\n  print(c.parent!.id)\n  print(c.parent == nil)\n}",
    );
    assert_eq!(run(&source), "1\nfalse\n-2\n-1\n");
}

#[test]
fn a_weak_member_does_not_keep_its_instance_alive() {
    let source = weak_family(
        "func main() {\n  let c = Child(parent: Parent(id: 1, child: nil), id: 2)\n  print(10)\n  print(c.parent == nil)\n}",
    );
    assert_eq!(run(&source), "-1\n10\ntrue\n-2\n");
}

#[test]
fn a_weak_member_becomes_nil_when_its_instance_is_released() {
    let source = weak_family(
        "func main() {\n  var p: Parent? = Parent(id: 1, child: nil)\n  let c = Child(parent: p, id: 2)\n  print(c.parent == nil)\n  p = nil\n  print(c.parent == nil)\n}",
    );
    assert_eq!(run(&source), "false\n-1\ntrue\n-2\n");
}

#[test]
fn a_parent_and_child_that_refer_to_each_other_are_both_released() {
    let source = weak_family(
        "func main() {\n  let p = Parent(id: 1, child: nil)\n  let c = Child(parent: p, id: 2)\n  p.child = c\n  print(10)\n  print(c.parent!.id)\n}",
    );
    assert_eq!(run(&source), "10\n1\n-1\n-2\n");
}

#[test]
fn a_weak_member_can_be_assigned_another_instance_or_nil() {
    let source = weak_family(
        "func main() {\n  let a = Parent(id: 1, child: nil)\n  let b = Parent(id: 3, child: nil)\n  let c = Child(parent: a, id: 2)\n  c.parent = b\n  print(c.parent!.id)\n  c.parent = nil\n  print(c.parent == nil)\n  c.parent = a\n  print(c.parent!.id)\n}",
    );
    assert_eq!(run(&source), "3\ntrue\n1\n-2\n-3\n-1\n");
}

#[test]
fn a_weak_member_can_be_bound_with_if_let_and_chained() {
    let source = weak_family(
        "func main() {\n  let p = Parent(id: 7, child: nil)\n  let c = Child(parent: p, id: 2)\n  if let q = c.parent {\n    print(q.id)\n  }\n  print(c.parent?.id ?? 0)\n  print(c.parent?.child?.id ?? 0 - 1)\n}",
    );
    assert_eq!(run(&source), "7\n7\n-1\n-2\n-7\n");
}

#[test]
fn a_binding_made_from_a_weak_member_keeps_the_instance_alive() {
    let source = weak_family(
        "func main() {\n  var p: Parent? = Parent(id: 1, child: nil)\n  let c = Child(parent: p, id: 2)\n  if let q = c.parent {\n    p = nil\n    print(10)\n    print(q.id)\n  }\n  print(20)\n  print(c.parent == nil)\n}",
    );
    assert_eq!(run(&source), "10\n1\n-1\n20\ntrue\n-2\n");
}

#[test]
fn a_weak_member_is_set_by_an_initializer() {
    let source = "class Parent {\n  var id: Int\n}\nclass Child {\n  weak var parent: Parent?\n  init(of p: Parent) { self.parent = p }\n}\nfunc main() {\n  let p = Parent(id: 8)\n  let c = Child(of: p)\n  print(c.parent!.id)\n}";
    assert_eq!(run(source), "8\n");
}

#[test]
fn a_weak_member_is_inherited_by_subclasses() {
    let source = "class Parent {\n  var id: Int\n}\nclass Child {\n  weak var parent: Parent?\n}\nclass Grandchild: Child {\n  var n: Int\n}\nfunc main() {\n  let p = Parent(id: 7)\n  let g = Grandchild(parent: p, n: 1)\n  print(g.parent!.id)\n}";
    assert_eq!(run(source), "7\n");
}

#[test]
fn releasing_the_holder_of_a_weak_reference_to_a_dead_instance_is_safe() {
    let source = weak_family(
        "func make() -> Child {\n  let p = Parent(id: 1, child: nil)\n  return Child(parent: p, id: 2)\n}\nfunc main() {\n  let c = make()\n  print(10)\n}",
    );
    assert_eq!(run(&source), "-1\n10\n-2\n");
}

#[test]
fn weak_needs_an_optional_class_type() {
    let plain = "class P { var x: Int }\nclass C {\n  weak var p: P\n}\nfunc main() { }";
    assert!(build_error(plain).contains("weak"));
    let int = "class C {\n  weak var n: Int?\n}\nfunc main() { }";
    assert!(build_error(int).contains("weak"));
}

#[test]
fn weak_needs_var() {
    let source = "class P { var x: Int }\nclass C {\n  weak let p: P?\n}\nfunc main() { }";
    assert!(build_error(source).contains("weak"));
}

#[test]
fn weak_is_rejected_on_a_struct_member() {
    let source = "class P { var x: Int }\nstruct S {\n  weak var p: P?\n}\nfunc main() { }";
    assert!(build_error(source).contains("weak"));
}

#[test]
fn a_member_cannot_be_both_weak_and_unowned() {
    let source = "class P { var x: Int }\nclass C {\n  weak unowned var p: P?\n}\nfunc main() { }";
    assert!(build_error(source).contains("weak"));
}

#[test]
fn a_switch_can_match_the_cases_of_an_optional() {
    let source = "func show(_ x: Int?) {\n  switch x {\n  case .some(let v): print(v)\n  case .none: print(0 - 1)\n  }\n}\nfunc main() {\n  show(4)\n  show(nil)\n}";
    assert_eq!(run(source), "4\n-1\n");
}

#[test]
fn a_switch_on_an_optional_may_use_a_default() {
    let source = "func main() {\n  let y: Int? = nil\n  switch y {\n  case .some(let v): print(v)\n  default: print(9)\n  }\n}";
    assert_eq!(run(source), "9\n");
}

#[test]
fn a_switch_on_an_optional_must_be_exhaustive() {
    let source =
        "func main() {\n  let y: Int? = 1\n  switch y {\n  case .some(let v): print(v)\n  }\n}";
    assert!(build_error(source).contains("exhaustive") || build_error(source).contains("none"));
}

#[test]
fn a_switch_on_an_optional_instance_binds_it_with_ownership() {
    let source = tracked(
        "func main() {\n  var t: Tracked? = Tracked(id: 1)\n  switch t {\n  case .some(let v):\n    t = nil\n    print(v.id)\n  case .none: print(0)\n  }\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n10\n");
}

#[test]
fn if_var_binds_a_variable_that_can_be_changed() {
    let source = "func main() {\n  let x: Int? = 4\n  if var v = x {\n    v += 1\n    print(v)\n  }\n  print(x!)\n}";
    assert_eq!(run(source), "5\n4\n");
}

#[test]
fn if_let_binds_a_constant() {
    let source = "func main() {\n  let x: Int? = 4\n  if let v = x {\n    v = 1\n  }\n}";
    assert!(build_error(source).contains("constant"));
}

#[test]
fn while_let_runs_until_the_value_is_nil() {
    let source = "class Node {\n  var value: Int\n  var next: Node?\n}\nfunc main() {\n  let c = Node(value: 3, next: nil)\n  let b = Node(value: 2, next: c)\n  let a = Node(value: 1, next: b)\n  var cursor: Node? = a\n  while let node = cursor {\n    print(node.value)\n    cursor = node.next\n  }\n  print(10)\n}";
    assert_eq!(run(source), "1\n2\n3\n10\n");
}

#[test]
fn while_let_does_not_run_for_nil() {
    let source =
        "func main() {\n  let x: Int? = nil\n  while let v = x { print(v) }\n  print(1)\n}";
    assert_eq!(run(source), "1\n");
}

#[test]
fn while_let_evaluates_its_subject_on_every_iteration() {
    let source = "func next(_ n: Int) -> Int? {\n  print(100 + n)\n  if n < 3 { return n }\n  return nil\n}\nfunc main() {\n  var i = 0\n  while let v = next(i) {\n    print(v)\n    i += 1\n  }\n}";
    assert_eq!(run(source), "100\n0\n101\n1\n102\n2\n103\n");
}

#[test]
fn while_let_releases_its_binding_on_every_iteration() {
    let source = tracked(
        "func make(_ n: Int) -> Tracked? {\n  if n < 3 { return Tracked(id: n + 1) }\n  return nil\n}\nfunc main() {\n  var i = 0\n  while let t = make(i) {\n    print(10 + i)\n    i += 1\n  }\n  print(99)\n}",
    );
    assert_eq!(run(&source), "10\n-1\n11\n-2\n12\n-3\n99\n");
}

#[test]
fn a_function_may_return_from_inside_while_let() {
    let source = "func first(_ x: Int?) -> Int {\n  while let v = x {\n    return v\n  }\n  return 0 - 1\n}\nfunc main() {\n  print(first(7))\n  print(first(nil))\n}";
    assert_eq!(run(source), "7\n-1\n");
}

#[test]
fn while_var_binds_a_variable_that_can_be_changed() {
    let source = "func main() {\n  var x: Int? = 2\n  while var v = x {\n    v += 10\n    print(v)\n    x = nil\n  }\n}";
    assert_eq!(run(source), "12\n");
}

#[test]
fn while_let_needs_an_optional() {
    assert!(
        build_error("func main() {\n  let a = 1\n  while let v = a { print(v) }\n}")
            .contains("optional")
    );
}

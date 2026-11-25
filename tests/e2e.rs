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

const CHAIN: &str = "class Engine {\n  var power: Int\n  func boost(_ n: Int) -> Int { return self.power + n }\n  func idle() { print(0) }\n}\nclass Car {\n  var engine: Engine\n  var spare: Engine?\n  var name: Int\n}\n";

fn chain(body: &str) -> String {
    format!("{CHAIN}{body}")
}

#[test]
fn an_optional_chain_continues_past_the_first_question_mark() {
    let source = chain(
        "func main() {\n  let car: Car? = Car(engine: Engine(power: 7), spare: nil, name: 1)\n  let none: Car? = nil\n  print(car?.engine.power ?? 0)\n  print(none?.engine.power ?? 0 - 1)\n}",
    );
    assert_eq!(run(&source), "7\n-1\n");
}

#[test]
fn an_optional_chain_continues_into_a_method_call() {
    let source = chain(
        "func main() {\n  let car: Car? = Car(engine: Engine(power: 7), spare: nil, name: 1)\n  print(car?.engine.boost(3) ?? 0)\n  car?.engine.idle()\n  let none: Car? = nil\n  none?.engine.idle()\n  print(none?.engine.boost(3) ?? 0 - 1)\n}",
    );
    assert_eq!(run(&source), "10\n0\n-1\n");
}

#[test]
fn an_optional_chain_may_pass_through_a_second_question_mark() {
    let source = chain(
        "func main() {\n  let car: Car? = Car(engine: Engine(power: 7), spare: Engine(power: 2), name: 1)\n  let bare: Car? = Car(engine: Engine(power: 7), spare: nil, name: 1)\n  print(car?.spare?.power ?? 0 - 1)\n  print(bare?.spare?.power ?? 0 - 1)\n  print(car?.spare?.boost(1) ?? 0 - 1)\n}",
    );
    assert_eq!(run(&source), "2\n-1\n3\n");
}

#[test]
fn an_optional_chain_flattens_an_optional_result() {
    let source = chain(
        "func main() {\n  let car: Car? = Car(engine: Engine(power: 7), spare: nil, name: 1)\n  print(car?.spare == nil)\n  let other: Car? = nil\n  print(other?.spare == nil)\n}",
    );
    assert_eq!(run(&source), "true\ntrue\n");
}

#[test]
fn an_optional_chain_may_unwrap_in_the_middle() {
    let source = chain(
        "func main() {\n  let car: Car? = Car(engine: Engine(power: 7), spare: Engine(power: 5), name: 1)\n  print(car?.spare!.power ?? 0)\n}",
    );
    assert_eq!(run(&source), "5\n");
}

#[test]
fn an_optional_chain_evaluates_its_subject_once_and_skips_the_rest_for_nil() {
    let source = "func noisy() -> Int {\n  print(100)\n  return 1\n}\nclass C {\n  var d: D\n}\nclass D {\n  var n: Int\n  func add(_ k: Int) -> Int { return self.n + k }\n}\nfunc make() -> C? {\n  print(50)\n  return C(d: D(n: 4))\n}\nfunc nothing() -> C? { return nil }\nfunc main() {\n  print(make()?.d.add(noisy()) ?? 0)\n  print(nothing()?.d.add(noisy()) ?? 0)\n}";
    assert_eq!(run(source), "50\n100\n5\n0\n");
}

#[test]
fn an_optional_chain_releases_what_it_creates() {
    let source = tracked(
        "class Box {\n  var item: Tracked\n}\nfunc make() -> Box? { return Box(item: Tracked(id: 1)) }\nfunc main() {\n  print(make()?.item.id ?? 0)\n  print(10)\n}",
    );
    assert_eq!(run(&source), "1\n-1\n10\n");
}

#[test]
fn a_chain_without_an_optional_before_the_first_link_is_rejected() {
    let source = chain(
        "func main() {\n  let car = Car(engine: Engine(power: 1), spare: nil, name: 1)\n  print(car?.engine.power ?? 0)\n}",
    );
    assert!(build_error(&source).contains("optional"));
}

#[test]
fn a_mutating_call_inside_an_optional_chain_is_rejected() {
    let source = "struct S {\n  var n: Int\n  mutating func bump() { self.n += 1 }\n}\nclass C {\n  var s: S\n}\nfunc main() {\n  let c: C? = C(s: S(n: 1))\n  c?.s.bump()\n}";
    assert!(build_error(source).contains("mutating"));
}

#[test]
fn guard_let_binds_a_name_for_the_rest_of_the_block() {
    let source = "func double(_ x: Int?) -> Int {\n  guard let v = x else { return 0 - 1 }\n  return v * 2\n}\nfunc main() {\n  print(double(4))\n  print(double(nil))\n}";
    assert_eq!(run(source), "8\n-1\n");
}

#[test]
fn guard_let_binding_is_visible_in_nested_blocks_after_it() {
    let source = "func f(_ x: Int?) -> Int {\n  guard let v = x else { return 0 }\n  if v > 1 {\n    return v + 100\n  }\n  return v\n}\nfunc main() {\n  print(f(5))\n  print(f(1))\n  print(f(nil))\n}";
    assert_eq!(run(source), "105\n1\n0\n");
}

#[test]
fn guard_let_binding_is_not_visible_in_its_else_body() {
    let source = "func f(_ x: Int?) -> Int {\n  guard let v = x else { return v }\n  return v\n}\nfunc main() { }";
    assert!(build_error(source).contains("cannot find"));
}

#[test]
fn guard_var_binds_a_variable_that_can_be_changed() {
    let source = "func f(_ x: Int?) -> Int {\n  guard var v = x else { return 0 }\n  v += 10\n  return v\n}\nfunc main() {\n  print(f(1))\n}";
    assert_eq!(run(source), "11\n");
}

#[test]
fn guard_with_a_condition_continues_only_when_it_holds() {
    let source = "func f(_ n: Int) -> Int {\n  guard n > 0 else { return 0 - 1 }\n  return n\n}\nfunc main() {\n  print(f(3))\n  print(f(0))\n}";
    assert_eq!(run(source), "3\n-1\n");
}

#[test]
fn guard_may_be_used_inside_a_loop_body() {
    let source = "func find(_ limit: Int) -> Int {\n  var i = 0\n  while i < 10 {\n    guard i < limit else { return i }\n    i += 1\n  }\n  return 99\n}\nfunc main() {\n  print(find(3))\n  print(find(50))\n}";
    assert_eq!(run(source), "3\n99\n");
}

#[test]
fn the_else_body_of_a_guard_must_leave_the_scope() {
    let source =
        "func f(_ x: Int?) {\n  guard let v = x else { print(0) }\n  print(v)\n}\nfunc main() { }";
    assert!(build_error(source).contains("guard"));
}

#[test]
fn the_else_body_of_a_guard_may_leave_through_every_branch() {
    let source = "func f(_ x: Int?, _ flag: Bool) -> Int {\n  guard let v = x else {\n    if flag { return 1 } else { return 2 }\n  }\n  return v\n}\nfunc main() {\n  print(f(nil, true))\n  print(f(nil, false))\n  print(f(7, true))\n}";
    assert_eq!(run(source), "1\n2\n7\n");
}

#[test]
fn guard_let_needs_an_optional() {
    let source = "func f(_ x: Int) -> Int {\n  guard let v = x else { return 0 }\n  return v\n}\nfunc main() { }";
    assert!(build_error(source).contains("optional"));
}

#[test]
fn guard_let_keeps_an_instance_until_the_end_of_the_block() {
    let source = tracked(
        "func f(_ t: Tracked?) -> Int {\n  guard let v = t else { return 0 }\n  print(10)\n  return v.id\n}\nfunc main() {\n  print(f(Tracked(id: 1)))\n  print(20)\n}",
    );
    assert_eq!(run(&source), "10\n1\n-1\n20\n");
}

#[test]
fn an_early_exit_from_guard_releases_what_was_declared_before_it() {
    let source = tracked(
        "func f(_ x: Int?) -> Int {\n  let a = Tracked(id: 1)\n  guard let v = x else { return 0 }\n  print(v)\n  return 1\n}\nfunc main() {\n  print(f(nil))\n  print(f(5))\n}",
    );
    assert_eq!(run(&source), "-1\n0\n5\n-1\n1\n");
}

const PETS: &str = "class Animal {\n  var legs: Int\n}\nclass Dog: Animal {\n  var tricks: Int\n}\nclass Puppy: Dog {\n  var age: Int\n}\nclass Cat: Animal {\n  var lives: Int\n}\n";

fn pets(body: &str) -> String {
    format!("{PETS}{body}")
}

#[test]
fn is_tests_the_class_of_an_instance_and_its_superclasses() {
    let source = pets(
        "func main() {\n  let d: Animal = Dog(legs: 4, tricks: 2)\n  let c: Animal = Cat(legs: 4, lives: 9)\n  print(d is Dog)\n  print(d is Cat)\n  print(c is Cat)\n  print(d is Animal)\n}",
    );
    assert_eq!(run(&source), "true\nfalse\ntrue\ntrue\n");
}

#[test]
fn is_accepts_instances_of_a_subclass_of_the_tested_class() {
    let source = pets(
        "func main() {\n  let p: Animal = Puppy(legs: 4, tricks: 1, age: 1)\n  print(p is Dog)\n  print(p is Puppy)\n  print(p is Cat)\n}",
    );
    assert_eq!(run(&source), "true\ntrue\nfalse\n");
}

#[test]
fn as_question_yields_the_instance_or_nil() {
    let source = pets(
        "func main() {\n  let a: Animal = Dog(legs: 4, tricks: 7)\n  let b: Animal = Cat(legs: 4, lives: 9)\n  print(a as? Dog == nil)\n  print((b as? Dog) == nil)\n  print((a as? Dog)!.tricks)\n}",
    );
    assert_eq!(run(&source), "false\ntrue\n7\n");
}

#[test]
fn as_question_works_with_if_let_and_chaining() {
    let source = pets(
        "func tricks(_ a: Animal) -> Int {\n  if let d = a as? Dog {\n    return d.tricks\n  }\n  return 0 - 1\n}\nfunc main() {\n  print(tricks(Dog(legs: 4, tricks: 3)))\n  print(tricks(Cat(legs: 4, lives: 9)))\n  let a: Animal = Dog(legs: 4, tricks: 5)\n  print((a as? Dog)?.tricks ?? 0)\n  print((a as? Cat)?.lives ?? 0)\n}",
    );
    assert_eq!(run(&source), "3\n-1\n5\n0\n");
}

#[test]
fn as_exclamation_returns_the_instance_or_traps() {
    let ok = pets(
        "func main() {\n  let a: Animal = Dog(legs: 4, tricks: 7)\n  let d = a as! Dog\n  print(d.tricks)\n}",
    );
    assert_eq!(run(&ok), "7\n");
    let bad = pets(
        "func main() {\n  let a: Animal = Cat(legs: 4, lives: 9)\n  let d = a as! Dog\n  print(d.tricks)\n}",
    );
    run_expecting_a_crash(&bad);
}

#[test]
fn a_cast_to_the_class_of_a_subclass_instance_accepts_deeper_instances() {
    let source = pets(
        "func main() {\n  let p: Animal = Puppy(legs: 4, tricks: 1, age: 2)\n  let d = p as! Dog\n  print(d.tricks)\n  let u = p as! Puppy\n  print(u.age)\n}",
    );
    assert_eq!(run(&source), "1\n2\n");
}

#[test]
fn a_cast_up_to_a_superclass_always_succeeds() {
    let source = pets(
        "func main() {\n  let d = Dog(legs: 4, tricks: 7)\n  print((d as? Animal)!.legs)\n  print(d is Animal)\n}",
    );
    assert_eq!(run(&source), "4\ntrue\n");
}

#[test]
fn a_cast_looks_through_an_optional_source() {
    let source = pets(
        "func main() {\n  let a: Animal? = Dog(legs: 4, tricks: 7)\n  let b: Animal? = Cat(legs: 4, lives: 9)\n  let n: Animal? = nil\n  print(a is Dog)\n  print(b is Dog)\n  print(n is Dog)\n  print((a as? Dog)!.tricks)\n  print((b as? Dog) == nil)\n  print((n as? Dog) == nil)\n}",
    );
    assert_eq!(run(&source), "true\nfalse\nfalse\n7\ntrue\ntrue\n");
}

#[test]
fn a_cast_result_keeps_the_instance_alive() {
    let source = "class Base {\n  var id: Int\n  deinit { print(0 - self.id) }\n}\nclass Derived: Base {\n  var n: Int\n}\nfunc main() {\n  var a: Base? = Derived(id: 1, n: 2)\n  let d = a as? Derived\n  a = nil\n  print(10)\n  print(d!.n)\n}";
    assert_eq!(run(source), "10\n2\n-1\n");
}

#[test]
fn a_failed_cast_creates_nothing_to_release() {
    let source = "class Base {\n  var id: Int\n  deinit { print(0 - self.id) }\n}\nclass A: Base {\n  var n: Int\n}\nclass B: Base {\n  var m: Int\n}\nfunc main() {\n  let x: Base = A(id: 1, n: 2)\n  let y = x as? B\n  print(y == nil)\n}";
    assert_eq!(run(source), "true\n-1\n");
}

#[test]
fn is_works_as_a_condition_and_dispatch_still_follows_the_instance() {
    let source = "class Shape {\n  func area() -> Int { return 1 }\n}\nclass Square: Shape {\n  override func area() -> Int { return 4 }\n}\nfunc main() {\n  let s: Shape = Square()\n  if s is Square {\n    print(s.area())\n  }\n  print((s as! Square).area())\n}";
    assert_eq!(run(source), "4\n4\n");
}

#[test]
fn a_cast_between_unrelated_classes_is_rejected() {
    let source = "class A { var x: Int }\nclass B { var y: Int }\nfunc main() {\n  let a = A(x: 1)\n  print(a is B)\n}";
    assert!(build_error(source).contains("unrelated"));
}

#[test]
fn a_cast_needs_a_class_on_both_sides() {
    assert!(build_error("func main() {\n  let n = 1\n  print(n is Int)\n}").contains("class"));
    let target = "class A { var x: Int }\nfunc main() {\n  let a = A(x: 1)\n  print(a is Int)\n}";
    assert!(build_error(target).contains("class"));
}

#[test]
fn break_leaves_the_innermost_while_loop() {
    let source = "func main() {\n  var i = 0\n  while true {\n    if i == 3 { break }\n    print(i)\n    i += 1\n  }\n  print(99)\n}";
    assert_eq!(run(source), "0\n1\n2\n99\n");
}

#[test]
fn continue_skips_the_rest_of_a_while_iteration() {
    let source = "func main() {\n  var i = 0\n  while i < 5 {\n    i += 1\n    if i == 3 { continue }\n    print(i)\n  }\n}";
    assert_eq!(run(source), "1\n2\n4\n5\n");
}

#[test]
fn break_and_continue_work_in_a_for_loop() {
    let source = "func main() {\n  for i in 0..<10 {\n    if i == 2 { continue }\n    if i == 5 { break }\n    print(i)\n  }\n  for j in 1...4 {\n    if j == 2 { continue }\n    print(j)\n  }\n}";
    assert_eq!(run(source), "0\n1\n3\n4\n1\n3\n4\n");
}

#[test]
fn continue_on_the_last_value_of_an_inclusive_range_ends_the_loop() {
    let source = "func main() {\n  for j in 1...3 {\n    if j == 3 { continue }\n    print(j)\n  }\n  print(9)\n}";
    assert_eq!(run(source), "1\n2\n9\n");
}

#[test]
fn break_affects_only_the_innermost_loop() {
    let source = "func main() {\n  for i in 0..<3 {\n    for j in 0..<3 {\n      if j == 1 { break }\n      print(i * 10 + j)\n    }\n  }\n}";
    assert_eq!(run(source), "0\n10\n20\n");
}

#[test]
fn a_labeled_break_leaves_the_named_loop() {
    let source = "func main() {\n  outer: for i in 0..<3 {\n    for j in 0..<3 {\n      if i == 1 && j == 1 { break outer }\n      print(i * 10 + j)\n    }\n  }\n  print(99)\n}";
    assert_eq!(run(source), "0\n1\n2\n10\n99\n");
}

#[test]
fn a_labeled_continue_starts_the_next_iteration_of_the_named_loop() {
    let source = "func main() {\n  outer: for i in 0..<3 {\n    for j in 0..<3 {\n      if j == 1 { continue outer }\n      print(i * 10 + j)\n    }\n  }\n}";
    assert_eq!(run(source), "0\n10\n20\n");
}

#[test]
fn break_inside_a_switch_leaves_only_the_switch() {
    let source = "enum Light { case red, green }\nfunc main() {\n  for i in 0..<2 {\n    let l = Light.red\n    switch l {\n    case .red:\n      print(1)\n      if i == 0 { break }\n      print(2)\n    case .green:\n      print(3)\n    }\n    print(10 + i)\n  }\n}";
    assert_eq!(run(source), "1\n10\n1\n2\n11\n");
}

#[test]
fn a_labeled_break_leaves_a_loop_from_inside_a_switch() {
    let source = "enum Light { case red, green }\nfunc main() {\n  loop: for i in 0..<3 {\n    let l = Light.red\n    switch l {\n    case .red:\n      if i == 1 { break loop }\n      print(i)\n    case .green:\n      print(3)\n    }\n  }\n  print(99)\n}";
    assert_eq!(run(source), "0\n99\n");
}

#[test]
fn continue_inside_a_switch_continues_the_enclosing_loop() {
    let source = "enum Light { case red, green }\nfunc main() {\n  for i in 0..<3 {\n    let l = Light.red\n    switch l {\n    case .red:\n      if i == 1 { continue }\n    case .green:\n      print(3)\n    }\n    print(i)\n  }\n}";
    assert_eq!(run(source), "0\n2\n");
}

#[test]
fn a_labeled_switch_can_be_left_from_a_nested_loop() {
    let source = "enum Light { case red }\nfunc main() {\n  let l = Light.red\n  pick: switch l {\n  case .red:\n    for i in 0..<5 {\n      if i == 2 { break pick }\n      print(i)\n    }\n    print(50)\n  }\n  print(99)\n}";
    assert_eq!(run(source), "0\n1\n99\n");
}

#[test]
fn break_and_continue_work_in_while_let() {
    let source = "func main() {\n  var i = 0\n  while let v = next(i) {\n    i += 1\n    if v == 1 { continue }\n    if v == 3 { break }\n    print(v)\n  }\n  print(99)\n}\nfunc next(_ n: Int) -> Int? {\n  if n < 10 { return n }\n  return nil\n}";
    assert_eq!(run(source), "0\n2\n99\n");
}

#[test]
fn guard_may_leave_a_loop_with_break_or_continue() {
    let source = "func main() {\n  for i in 0..<5 {\n    guard i != 1 else { continue }\n    guard i < 3 else { break }\n    print(i)\n  }\n  print(99)\n}";
    assert_eq!(run(source), "0\n2\n99\n");
}

#[test]
fn guard_let_may_leave_a_loop_with_break() {
    let source = "func main() {\n  var i = 0\n  while true {\n    guard let v = pick(i) else { break }\n    print(v)\n    i += 1\n  }\n}\nfunc pick(_ n: Int) -> Int? {\n  if n < 2 { return n + 5 }\n  return nil\n}";
    assert_eq!(run(source), "5\n6\n");
}

#[test]
fn break_releases_the_variables_of_the_loop_body() {
    let source = tracked(
        "func main() {\n  var i = 0\n  while true {\n    let t = Tracked(id: i + 1)\n    if i == 1 { break }\n    print(i)\n    i += 1\n  }\n  print(99)\n}",
    );
    assert_eq!(run(&source), "0\n-1\n-2\n99\n");
}

#[test]
fn continue_releases_the_variables_of_the_loop_body() {
    let source = tracked(
        "func main() {\n  for i in 0..<3 {\n    let t = Tracked(id: i + 1)\n    if i == 1 { continue }\n    print(i)\n  }\n}",
    );
    assert_eq!(run(&source), "0\n-1\n-2\n2\n-3\n");
}

#[test]
fn a_labeled_break_releases_the_variables_of_every_loop_it_leaves() {
    let source = tracked(
        "func main() {\n  outer: for i in 0..<2 {\n    let a = Tracked(id: 1)\n    for j in 0..<2 {\n      let b = Tracked(id: 2)\n      break outer\n    }\n  }\n  print(99)\n}",
    );
    assert_eq!(run(&source), "-2\n-1\n99\n");
}

#[test]
fn continue_re_evaluates_a_while_condition_that_owns_temporaries() {
    let source = tracked(
        "func make(_ n: Int) -> Tracked { return Tracked(id: n) }\nfunc main() {\n  var i = 1\n  while make(i).id < 4 {\n    i += 1\n    if i == 2 { continue }\n    print(100 + i)\n  }\n}",
    );
    assert_eq!(run(&source), "-1\n-2\n103\n-3\n104\n-4\n");
}

#[test]
fn a_function_may_end_after_a_loop_that_only_breaks() {
    let source =
        "func f() -> Int {\n  while true { break }\n  return 7\n}\nfunc main() { print(f()) }";
    assert_eq!(run(source), "7\n");
}

#[test]
fn break_outside_a_loop_or_switch_is_rejected() {
    assert!(build_error("func main() { break }").contains("break"));
}

#[test]
fn continue_outside_a_loop_is_rejected() {
    assert!(build_error("func main() { continue }").contains("continue"));
    let in_switch = "enum E { case a }\nfunc main() {\n  let e = E.a\n  switch e {\n  case .a:\n    continue\n  }\n}";
    assert!(build_error(in_switch).contains("continue"));
}

#[test]
fn a_label_that_names_nothing_is_rejected() {
    assert!(build_error("func main() {\n  while true { break nowhere }\n}").contains("nowhere"));
}

#[test]
fn continue_cannot_target_a_labeled_switch() {
    let source = "enum E { case a }\nfunc main() {\n  let e = E.a\n  pick: switch e {\n  case .a:\n    while true { continue pick }\n  }\n}";
    assert!(build_error(source).contains("pick"));
}

#[test]
fn a_label_name_cannot_be_reused_by_a_nested_statement() {
    let source =
        "func main() {\n  a: while true {\n    a: while true { break a }\n    break a\n  }\n}";
    assert!(build_error(source).contains("label"));
}

#[test]
fn strings_with_the_same_text_are_equal() {
    let source = "func main() {\n  let a = \"hello\"\n  let b = \"hel\"\n  print(a == \"hello\")\n  print(a == b)\n  print(a != b)\n  print(\"\" == \"\")\n  print(\"ab\" == \"abc\")\n  print(\"abc\" == \"ab\")\n  print(\"abc\" == \"abd\")\n}";
    assert_eq!(
        run(source),
        "true\nfalse\ntrue\ntrue\nfalse\nfalse\nfalse\n"
    );
}

#[test]
fn strings_are_compared_by_content_and_not_by_address() {
    let source = "func same(_ a: String, _ b: String) -> Bool { return a == b }\nfunc main() {\n  let x = \"abc\"\n  print(same(x, \"abc\"))\n  print(same(x, \"abd\"))\n}";
    assert_eq!(run(source), "true\nfalse\n");
}

#[test]
fn a_string_cannot_be_compared_with_another_type() {
    assert!(build_error("func main() { print(\"a\" == 1) }").contains("expected"));
}

const POINT_DECL: &str = "struct Point {\n  var x: Int\n  var y: Int\n}\n";

#[test]
fn structs_with_equal_members_are_equal() {
    let source = format!(
        "{POINT_DECL}func main() {{\n  let a = Point(x: 1, y: 2)\n  let b = Point(x: 1, y: 2)\n  let c = Point(x: 1, y: 3)\n  print(a == b)\n  print(a == c)\n  print(a != c)\n  print(a != b)\n}}"
    );
    assert_eq!(run(&source), "true\nfalse\ntrue\nfalse\n");
}

#[test]
fn struct_equality_compares_nested_structs_strings_and_enums() {
    let source = "enum Color { case red, green }\nstruct Inner {\n  var n: Int\n  var ok: Bool\n}\nstruct Outer {\n  var name: String\n  var color: Color\n  var inner: Inner\n}\nfunc main() {\n  let a = Outer(name: \"a\", color: Color.red, inner: Inner(n: 1, ok: true))\n  let b = Outer(name: \"a\", color: Color.red, inner: Inner(n: 1, ok: true))\n  let c = Outer(name: \"b\", color: Color.red, inner: Inner(n: 1, ok: true))\n  let d = Outer(name: \"a\", color: Color.green, inner: Inner(n: 1, ok: true))\n  let e = Outer(name: \"a\", color: Color.red, inner: Inner(n: 2, ok: true))\n  print(a == b)\n  print(a == c)\n  print(a == d)\n  print(a == e)\n}";
    assert_eq!(run(source), "true\nfalse\nfalse\nfalse\n");
}

#[test]
fn struct_equality_evaluates_each_operand_once() {
    let source = format!(
        "{POINT_DECL}func make(_ n: Int) -> Point {{\n  print(n)\n  return Point(x: 1, y: 2)\n}}\nfunc main() {{\n  print(make(1) == make(2))\n}}"
    );
    assert_eq!(run(&source), "1\n2\ntrue\n");
}

#[test]
fn struct_equality_works_in_conditions_and_on_mutated_values() {
    let source = format!(
        "{POINT_DECL}func main() {{\n  var p = Point(x: 0, y: 0)\n  let goal = Point(x: 2, y: 0)\n  while p != goal {{\n    p.x += 1\n  }}\n  print(p == goal)\n}}"
    );
    assert_eq!(run(&source), "true\n");
}

#[test]
fn a_struct_with_a_class_member_cannot_be_compared() {
    let source = "class Box { var v: Int }\nstruct Holder {\n  var b: Box\n}\nfunc main() {\n  let a = Holder(b: Box(v: 1))\n  print(a == a)\n}";
    assert!(build_error(source).contains("equality is not supported"));
}

#[test]
fn a_struct_with_a_payload_enum_member_cannot_be_compared() {
    let source = "enum Shape { case circle(Int), empty }\nstruct Holder {\n  var s: Shape\n}\nfunc main() {\n  let a = Holder(s: Shape.empty)\n  print(a == a)\n}";
    assert!(build_error(source).contains("equality is not supported"));
}

#[test]
fn structs_of_different_types_cannot_be_compared() {
    let source = format!(
        "{POINT_DECL}struct Other {{\n  var x: Int\n  var y: Int\n}}\nfunc main() {{\n  print(Point(x: 1, y: 2) == Other(x: 1, y: 2))\n}}"
    );
    assert!(build_error(&source).contains("expected"));
}

#[test]
fn an_int_switch_runs_the_case_that_lists_the_value() {
    let source = "func name(_ n: Int) -> Int {\n  switch n {\n  case 1, 2:\n    return 10\n  case 3:\n    return 30\n  default:\n    return 0\n  }\n}\nfunc main() {\n  print(name(1))\n  print(name(2))\n  print(name(3))\n  print(name(4))\n}";
    assert_eq!(run(source), "10\n10\n30\n0\n");
}

#[test]
fn an_int_switch_accepts_negative_literals() {
    let source = "func main() {\n  let n = 0 - 5\n  switch n {\n  case -5:\n    print(1)\n  default:\n    print(2)\n  }\n}";
    assert_eq!(run(source), "1\n");
}

#[test]
fn an_int_switch_accepts_closed_and_half_open_ranges() {
    let source = "func kind(_ n: Int) -> Int {\n  switch n {\n  case 0..<3:\n    return 1\n  case 3...5:\n    return 2\n  case -9...-1:\n    return 3\n  default:\n    return 4\n  }\n}\nfunc main() {\n  print(kind(0))\n  print(kind(2))\n  print(kind(3))\n  print(kind(5))\n  print(kind(6))\n  print(kind(-1))\n  print(kind(-10))\n}";
    assert_eq!(run(source), "1\n1\n2\n2\n4\n3\n4\n");
}

#[test]
fn the_first_matching_case_of_a_switch_wins() {
    let source = "func main() {\n  switch 2 {\n  case 1...3:\n    print(1)\n  case 2:\n    print(2)\n  default:\n    print(3)\n  }\n}";
    assert_eq!(run(source), "1\n");
}

#[test]
fn a_string_switch_compares_by_content() {
    let source = "func code(_ s: String) -> Int {\n  switch s {\n  case \"a\", \"b\":\n    return 1\n  case \"abc\":\n    return 2\n  default:\n    return 0\n  }\n}\nfunc main() {\n  print(code(\"b\"))\n  print(code(\"abc\"))\n  print(code(\"ab\"))\n}";
    assert_eq!(run(source), "1\n2\n0\n");
}

#[test]
fn a_bool_switch_is_exhaustive_with_true_and_false() {
    let source = "func word(_ b: Bool) -> Int {\n  switch b {\n  case true:\n    return 1\n  case false:\n    return 2\n  }\n}\nfunc main() {\n  print(word(true))\n  print(word(false))\n}";
    assert_eq!(run(source), "1\n2\n");
}

#[test]
fn a_switch_evaluates_its_subject_once() {
    let source = "func next() -> Int {\n  print(7)\n  return 2\n}\nfunc main() {\n  switch next() {\n  case 1:\n    print(1)\n  case 2:\n    print(2)\n  default:\n    print(3)\n  }\n}";
    assert_eq!(run(source), "7\n2\n");
}

#[test]
fn break_leaves_a_value_switch() {
    let source = "func main() {\n  switch 1 {\n  case 1:\n    print(1)\n    break\n  default:\n    print(2)\n  }\n  print(3)\n}";
    assert_eq!(run(source), "1\n3\n");
}

#[test]
fn an_int_switch_needs_a_default() {
    let source = "func main() {\n  switch 1 {\n  case 1:\n    print(1)\n  }\n}";
    assert!(build_error(source).contains("default"));
}

#[test]
fn a_string_switch_needs_a_default() {
    let source = "func main() {\n  switch \"a\" {\n  case \"a\":\n    print(1)\n  }\n}";
    assert!(build_error(source).contains("default"));
}

#[test]
fn a_bool_switch_must_cover_both_values_without_a_default() {
    let source = "func main() {\n  switch true {\n  case true:\n    print(1)\n  }\n}";
    assert!(build_error(source).contains("exhaustive"));
}

#[test]
fn a_case_value_must_have_the_type_of_the_subject() {
    let source = "func main() {\n  switch 1 {\n  case \"a\":\n    print(1)\n  default:\n    print(2)\n  }\n}";
    assert!(build_error(source).contains("expected"));
}

#[test]
fn a_case_value_must_be_a_literal() {
    let source = "func main() {\n  let k = 1\n  switch 1 {\n  case k:\n    print(1)\n  default:\n    print(2)\n  }\n}";
    assert!(build_error(source).contains("literal"));
}

#[test]
fn ranges_are_allowed_only_in_int_switches() {
    let source = "func main() {\n  switch \"a\" {\n  case 1...2:\n    print(1)\n  default:\n    print(2)\n  }\n}";
    assert!(!build_error(source).is_empty());
}

#[test]
fn an_enum_switch_rejects_value_cases() {
    let source = "enum E { case a }\nfunc main() {\n  let e = E.a\n  switch e {\n  case 1:\n    print(1)\n  default:\n    print(2)\n  }\n}";
    assert!(!build_error(source).is_empty());
}

#[test]
fn a_value_switch_rejects_enum_patterns() {
    let source = "enum E { case a }\nfunc main() {\n  switch 1 {\n  case .a:\n    print(1)\n  default:\n    print(2)\n  }\n}";
    assert!(!build_error(source).is_empty());
}

const SHAPE_PROTOCOL: &str = "protocol Shape {\n  func area() -> Int\n}\nstruct Square: Shape {\n  var side: Int\n  func area() -> Int { return side * side }\n}\nstruct Rect: Shape {\n  var w: Int\n  var h: Int\n  func area() -> Int { return w * h }\n}\n";

#[test]
fn a_generic_function_calls_a_requirement_of_its_constraint() {
    let source = format!(
        "{SHAPE_PROTOCOL}func total<T: Shape>(_ a: T, _ b: T) -> Int {{\n  return a.area() + b.area()\n}}\nfunc main() {{\n  print(total(Square(side: 2), Square(side: 3)))\n  print(total(Rect(w: 2, h: 5), Rect(w: 1, h: 1)))\n}}"
    );
    assert_eq!(run(&source), "13\n11\n");
}

#[test]
fn a_class_and_an_enum_can_conform_to_a_protocol() {
    let source = "protocol Named {\n  func code() -> Int\n}\nclass Dog: Named {\n  var n: Int\n  func code() -> Int { return n }\n}\nenum Color: Named {\n  case red, green\n  func code() -> Int {\n    switch self {\n    case .red: return 1\n    case .green: return 2\n    }\n  }\n}\nfunc show<T: Named>(_ x: T) {\n  print(x.code())\n}\nfunc main() {\n  show(Dog(n: 7))\n  show(Color.green)\n}";
    assert_eq!(run(source), "7\n2\n");
}

#[test]
fn a_subclass_conforms_through_its_superclass_and_dispatches_dynamically() {
    let source = "protocol Named {\n  func code() -> Int\n}\nclass Animal: Named {\n  func code() -> Int { return 1 }\n}\nclass Cat: Animal {\n  override func code() -> Int { return 2 }\n}\nfunc show<T: Named>(_ x: T) {\n  print(x.code())\n}\nfunc main() {\n  let a: Animal = Cat()\n  show(a)\n  show(Animal())\n  show(Cat())\n}";
    assert_eq!(run(source), "2\n1\n2\n");
}

#[test]
fn a_requirement_may_take_arguments_and_the_self_type() {
    let source = "protocol Joinable {\n  func join(_ other: Self, by: Int) -> Self\n}\nstruct Num: Joinable {\n  var v: Int\n  func join(_ other: Num, by: Int) -> Num { return Num(v: v + other.v + by) }\n}\nfunc combine<T: Joinable>(_ a: T, _ b: T) -> T {\n  return a.join(b, by: 100)\n}\nfunc main() {\n  print(combine(Num(v: 1), Num(v: 2)).v)\n}";
    assert_eq!(run(source), "103\n");
}

#[test]
fn a_mutating_requirement_changes_the_caller_variable() {
    let source = "protocol Counter {\n  mutating func bump()\n  func value() -> Int\n}\nstruct Tally: Counter {\n  var n: Int\n  mutating func bump() { n += 1 }\n  func value() -> Int { return n }\n}\nfunc twice<T: Counter>(_ c: T) -> Int {\n  var x = c\n  x.bump()\n  x.bump()\n  return x.value()\n}\nfunc main() {\n  print(twice(Tally(n: 5)))\n}";
    assert_eq!(run(source), "7\n");
}

#[test]
fn a_constraint_may_list_several_protocols() {
    let source = "protocol A {\n  func a() -> Int\n}\nprotocol B {\n  func b() -> Int\n}\nstruct Both: A, B {\n  var x: Int\n  func a() -> Int { return x }\n  func b() -> Int { return x * 2 }\n}\nfunc sum<T: A & B>(_ v: T) -> Int {\n  return v.a() + v.b()\n}\nfunc main() {\n  print(sum(Both(x: 4)))\n}";
    assert_eq!(run(source), "12\n");
}

#[test]
fn a_constrained_value_can_be_passed_to_another_constrained_function() {
    let source = format!(
        "{SHAPE_PROTOCOL}func area_of<T: Shape>(_ s: T) -> Int {{ return s.area() }}\nfunc twice<T: Shape>(_ s: T) -> Int {{\n  return area_of(s) * 2\n}}\nfunc main() {{\n  print(twice(Square(side: 3)))\n}}"
    );
    assert_eq!(run(&source), "18\n");
}

#[test]
fn a_generic_type_may_constrain_its_parameter() {
    let source = format!(
        "{SHAPE_PROTOCOL}struct Holder<T: Shape> {{\n  var item: T\n  func double() -> Int {{ return item.area() * 2 }}\n}}\nfunc main() {{\n  let h = Holder(item: Square(side: 3))\n  print(h.double())\n  let r = Holder(item: Rect(w: 2, h: 3))\n  print(r.double())\n}}"
    );
    assert_eq!(run(&source), "18\n12\n");
}

#[test]
fn a_generic_method_may_have_a_constrained_parameter() {
    let source = format!(
        "{SHAPE_PROTOCOL}struct Scale {{\n  var k: Int\n  func apply<T: Shape>(_ s: T) -> Int {{ return s.area() * k }}\n}}\nfunc main() {{\n  print(Scale(k: 3).apply(Square(side: 2)))\n}}"
    );
    assert_eq!(run(&source), "12\n");
}

#[test]
fn equatable_gives_a_type_parameter_equality() {
    let source = "enum Color { case red, green }\nfunc same<T: Equatable>(_ a: T, _ b: T) -> Bool { return a == b }\nfunc differ<T: Equatable>(_ a: T, _ b: T) -> Bool { return a != b }\nfunc main() {\n  print(same(1, 1))\n  print(same(1, 2))\n  print(same(\"a\", \"a\"))\n  print(differ(\"a\", \"b\"))\n  print(same(true, false))\n  print(same(Color.red, Color.red))\n  print(differ(Color.red, Color.green))\n}";
    assert_eq!(run(source), "true\nfalse\ntrue\ntrue\nfalse\ntrue\ntrue\n");
}

#[test]
fn a_struct_that_declares_equatable_is_compared_member_by_member() {
    let source = "struct P: Equatable {\n  var x: Int\n  var name: String\n}\nfunc same<T: Equatable>(_ a: T, _ b: T) -> Bool { return a == b }\nfunc main() {\n  print(same(P(x: 1, name: \"a\"), P(x: 1, name: \"a\")))\n  print(same(P(x: 1, name: \"a\"), P(x: 1, name: \"b\")))\n}";
    assert_eq!(run(source), "true\nfalse\n");
}

#[test]
fn comparable_gives_a_type_parameter_ordering_and_equality() {
    let source = "func largest<T: Comparable>(_ a: T, _ b: T) -> T {\n  if a > b { return a }\n  return b\n}\nfunc inside<T: Comparable>(_ x: T, _ lo: T, _ hi: T) -> Bool {\n  return x >= lo && x <= hi && x != hi\n}\nfunc smaller<T: Comparable>(_ a: T, _ b: T) -> Bool { return a < b }\nfunc main() {\n  print(largest(3, 9))\n  print(largest(9, 3))\n  print(inside(5, 1, 9))\n  print(inside(9, 1, 9))\n  print(smaller(1, 2))\n}";
    assert_eq!(run(source), "9\n9\ntrue\nfalse\ntrue\n");
}

#[test]
fn protocol_calls_in_a_generic_function_are_instantiated_per_type() {
    let source = format!(
        "{SHAPE_PROTOCOL}func area_of<T: Shape>(_ s: T) -> Int {{ return s.area() }}\nfunc main() {{\n  print(area_of(Square(side: 2)))\n  print(area_of(Rect(w: 3, h: 4)))\n  print(area_of(Square(side: 5)))\n}}"
    );
    assert_eq!(run(&source), "4\n12\n25\n");
}

#[test]
fn a_type_that_does_not_conform_is_rejected_at_the_call() {
    let source = format!(
        "{SHAPE_PROTOCOL}struct Other {{\n  var x: Int\n}}\nfunc area_of<T: Shape>(_ s: T) -> Int {{ return s.area() }}\nfunc main() {{\n  print(area_of(Other(x: 1)))\n}}"
    );
    let message = build_error(&source);
    assert!(
        message.contains("Other") && message.contains("Shape"),
        "{message}"
    );
}

#[test]
fn a_conforming_type_must_implement_every_requirement() {
    let source = "protocol P {\n  func a() -> Int\n  func b() -> Int\n}\nstruct S: P {\n  var x: Int\n  func a() -> Int { return x }\n}\nfunc main() { }";
    assert!(build_error(source).contains("'b'"));
}

#[test]
fn an_implementation_must_match_the_requirement_signature() {
    let wrong_result = "protocol P {\n  func a() -> Int\n}\nstruct S: P {\n  var x: Int\n  func a() -> Bool { return true }\n}\nfunc main() { }";
    assert!(build_error(wrong_result).contains("'a'"));
    let wrong_label = "protocol P {\n  func a(x: Int) -> Int\n}\nstruct S: P {\n  var x: Int\n  func a(_ x: Int) -> Int { return x }\n}\nfunc main() { }";
    assert!(build_error(wrong_label).contains("'a'"));
    let not_mutating = "protocol P {\n  mutating func a()\n}\nstruct S: P {\n  var x: Int\n  func a() { }\n}\nfunc main() { }";
    assert!(build_error(not_mutating).contains("mutating"));
}

#[test]
fn a_conformance_must_name_a_protocol() {
    let unknown = "struct S: Nothing {\n  var x: Int\n}\nfunc main() { }";
    assert!(build_error(unknown).contains("Nothing"));
    let class_name = "class A {\n  var x: Int\n}\nstruct S: A {\n  var x: Int\n}\nfunc main() { }";
    assert!(build_error(class_name).contains("not a protocol"));
}

#[test]
fn a_constraint_must_name_a_protocol() {
    let source = "struct A {\n  var x: Int\n}\nfunc f<T: A>(_ x: T) { }\nfunc main() { }";
    assert!(build_error(source).contains("not a protocol"));
}

#[test]
fn a_method_outside_the_constraint_cannot_be_called() {
    let source = "protocol P {\n  func a() -> Int\n}\nfunc f<T: P>(_ x: T) -> Int { return x.b() }\nfunc main() { }";
    assert!(build_error(source).contains("'b'"));
    let none = "func f<T>(_ x: T) -> Int { return x.a() }\nfunc main() { }";
    assert!(!build_error(none).is_empty());
}

#[test]
fn an_operator_needs_the_matching_standard_protocol() {
    let none = "func f<T>(_ a: T, _ b: T) -> Bool { return a == b }\nfunc main() { }";
    assert!(!build_error(none).is_empty());
    let equatable_only =
        "func f<T: Equatable>(_ a: T, _ b: T) -> Bool { return a < b }\nfunc main() { }";
    assert!(!build_error(equatable_only).is_empty());
}

#[test]
fn a_constrained_value_cannot_flow_into_a_stronger_constraint() {
    let source = "protocol P {\n  func a() -> Int\n}\nprotocol Q {\n  func b() -> Int\n}\nfunc needs_q<T: Q>(_ x: T) -> Int { return x.b() }\nfunc f<T: P>(_ x: T) -> Int { return needs_q(x) }\nfunc main() { }";
    assert!(build_error(source).contains("Q"));
}

#[test]
fn a_string_is_not_comparable_yet() {
    let source = "func f<T: Comparable>(_ a: T, _ b: T) -> Bool { return a < b }\nfunc main() {\n  print(f(\"a\", \"b\"))\n}";
    assert!(build_error(source).contains("Comparable"));
}

#[test]
fn equatable_cannot_be_declared_by_a_type_with_a_class_member() {
    let source =
        "class Box {\n  var v: Int\n}\nstruct S: Equatable {\n  var b: Box\n}\nfunc main() { }";
    assert!(build_error(source).contains("equality is not supported"));
}

#[test]
fn a_protocol_name_cannot_be_reused() {
    let twice = "protocol P {\n  func a()\n}\nprotocol P {\n  func b()\n}\nfunc main() { }";
    assert!(build_error(twice).contains("duplicate"));
    let builtin = "protocol Equatable {\n  func a()\n}\nfunc main() { }";
    assert!(build_error(builtin).contains("Equatable"));
}

#[test]
fn a_protocol_without_self_requirements_can_be_used_as_a_type() {
    let source = "protocol P {\n  func a() -> Int\n}\nfunc f(_ x: P) { }\nfunc main() { }";
    assert_eq!(run(source), "");
}

const CONTAINER: &str = "protocol Container {\n  associatedtype Item\n  func first() -> Item\n  func count() -> Int\n}\nstruct IntBox: Container {\n  typealias Item = Int\n  var v: Int\n  func first() -> Int { return v }\n  func count() -> Int { return 1 }\n}\nstruct Label: Container {\n  typealias Item = String\n  var text: String\n  func first() -> String { return text }\n  func count() -> Int { return 2 }\n}\n";

#[test]
fn a_function_may_return_the_associated_type_of_its_parameter() {
    let source = format!(
        "{CONTAINER}func head<C: Container>(_ c: C) -> C.Item {{ return c.first() }}\nfunc main() {{\n  print(head(IntBox(v: 5)))\n  print(head(Label(text: \"hi\")))\n}}"
    );
    assert_eq!(run(&source), "5\nhi\n");
}

#[test]
fn an_associated_type_may_be_a_parameter_type() {
    let source = "protocol Sink {\n  associatedtype Item\n  func put(_ x: Item) -> Int\n}\nstruct Adder: Sink {\n  typealias Item = Int\n  var base: Int\n  func put(_ x: Int) -> Int { return base + x }\n}\nfunc feed<S: Sink>(_ s: S, _ x: S.Item) -> Int { return s.put(x) }\nfunc main() {\n  print(feed(Adder(base: 10), 5))\n}";
    assert_eq!(run(source), "15\n");
}

#[test]
fn a_local_may_have_the_associated_type_as_its_annotation() {
    let source = format!(
        "{CONTAINER}func twice<C: Container>(_ c: C) -> Int {{\n  let x: C.Item = c.first()\n  let y: C.Item = c.first()\n  return c.count() * 2\n}}\nfunc main() {{\n  print(twice(IntBox(v: 1)))\n}}"
    );
    assert_eq!(run(&source), "2\n");
}

#[test]
fn a_generic_type_may_bind_the_associated_type_to_its_parameter() {
    let source = "protocol Holder {\n  associatedtype Item\n  func get() -> Item\n}\nstruct Box<T>: Holder {\n  typealias Item = T\n  var value: T\n  func get() -> T { return value }\n}\nfunc open<H: Holder>(_ h: H) -> H.Item { return h.get() }\nfunc main() {\n  print(open(Box(value: 7)))\n  print(open(Box(value: true)))\n}";
    assert_eq!(run(source), "7\ntrue\n");
}

#[test]
fn a_class_and_an_enum_can_bind_associated_types() {
    let source = "protocol Source {\n  associatedtype Out\n  func make() -> Out\n}\nclass Counter: Source {\n  typealias Out = Int\n  var n: Int\n  func make() -> Int { return n }\n}\nenum Flag: Source {\n  typealias Out = Bool\n  case on\n  func make() -> Bool { return true }\n}\nfunc run<S: Source>(_ s: S) -> S.Out { return s.make() }\nfunc main() {\n  print(run(Counter(n: 3)))\n  print(run(Flag.on))\n}";
    assert_eq!(run(source), "3\ntrue\n");
}

#[test]
fn the_associated_type_flows_through_another_constrained_function() {
    let source = format!(
        "{CONTAINER}func head<C: Container>(_ c: C) -> C.Item {{ return c.first() }}\nfunc again<D: Container>(_ d: D) -> D.Item {{ return head(d) }}\nfunc main() {{\n  print(again(IntBox(v: 9)))\n}}"
    );
    assert_eq!(run(&source), "9\n");
}

#[test]
fn a_struct_member_may_have_the_associated_type() {
    let source = format!(
        "{CONTAINER}struct Wrapper<C: Container> {{\n  var item: C.Item\n  var tag: Int\n}}\nfunc main() {{\n  let w = Wrapper<IntBox>(item: 4, tag: 1)\n  print(w.item + w.tag)\n}}"
    );
    assert_eq!(run(&source), "5\n");
}

#[test]
fn a_type_may_use_its_own_typealias_in_a_member_signature() {
    let source = "protocol Container {\n  associatedtype Item\n  func first() -> Item\n}\nstruct IntBox: Container {\n  typealias Item = Int\n  var v: Int\n  func first() -> Item { return v }\n}\nfunc main() {\n  print(IntBox(v: 6).first())\n}";
    assert_eq!(run(source), "6\n");
}

#[test]
fn a_conforming_type_must_bind_every_associated_type() {
    let source = "protocol Container {\n  associatedtype Item\n  func count() -> Int\n}\nstruct S: Container {\n  var v: Int\n  func count() -> Int { return 1 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("'Item'"));
}

#[test]
fn a_method_must_agree_with_the_bound_associated_type() {
    let source = "protocol Container {\n  associatedtype Item\n  func first() -> Item\n}\nstruct S: Container {\n  typealias Item = Int\n  func first() -> Bool { return true }\n}\nfunc main() { }";
    assert!(build_error(source).contains("'first'"));
}

#[test]
fn an_unknown_associated_type_is_rejected() {
    let source = format!(
        "{CONTAINER}func f<C: Container>(_ c: C) -> C.Nope {{ return c.first() }}\nfunc main() {{ }}"
    );
    assert!(build_error(&source).contains("Nope"));
    let unconstrained = "func f<C>(_ c: C) -> C.Item { return c }\nfunc main() { }";
    assert!(build_error(unconstrained).contains("Item"));
}

#[test]
fn the_associated_type_is_opaque_inside_a_generic_body() {
    let source = format!(
        "{CONTAINER}func bad<C: Container>(_ c: C) -> Int {{ return c.first() }}\nfunc main() {{ }}"
    );
    assert!(build_error(&source).contains("expected"));
}

#[test]
fn an_associated_type_name_cannot_be_declared_twice() {
    let source = "protocol P {\n  associatedtype Item\n  associatedtype Item\n}\nfunc main() { }";
    assert!(build_error(source).contains("Item"));
}

const PROTOCOL_SHAPES: &str = "protocol Shape {\n  func area() -> Int\n}\nstruct Square: Shape {\n  var side: Int\n  func area() -> Int { return side * side }\n}\nstruct Rect: Shape {\n  var w: Int\n  var h: Int\n  func area() -> Int { return w * h }\n}\nclass Circle: Shape {\n  var r: Int\n  func area() -> Int { return 3 * r * r }\n}\nenum Unit: Shape {\n  case one\n  func area() -> Int { return 1 }\n}\n";

#[test]
fn a_protocol_value_holds_a_struct_a_class_or_an_enum() {
    let source = format!(
        "{PROTOCOL_SHAPES}func main() {{\n  let a: Shape = Square(side: 3)\n  let b: Shape = Rect(w: 2, h: 5)\n  let c: Shape = Circle(r: 2)\n  let d: Shape = Unit.one\n  print(a.area())\n  print(b.area())\n  print(c.area())\n  print(d.area())\n}}"
    );
    assert_eq!(run(&source), "9\n10\n12\n1\n");
}

#[test]
fn a_function_may_take_and_return_a_protocol_value() {
    let source = format!(
        "{PROTOCOL_SHAPES}func show(_ s: Shape) {{ print(s.area()) }}\nfunc make(_ n: Int) -> Shape {{\n  if n == 0 {{ return Square(side: 4) }}\n  return Circle(r: n)\n}}\nfunc main() {{\n  show(Rect(w: 1, h: 7))\n  show(make(0))\n  show(make(3))\n}}"
    );
    assert_eq!(run(&source), "7\n16\n27\n");
}

#[test]
fn a_protocol_variable_may_hold_a_different_type_after_an_assignment() {
    let source = format!(
        "{PROTOCOL_SHAPES}func main() {{\n  var s: Shape = Square(side: 2)\n  print(s.area())\n  s = Circle(r: 1)\n  print(s.area())\n  s = Rect(w: 3, h: 3)\n  print(s.area())\n}}"
    );
    assert_eq!(run(&source), "4\n3\n9\n");
}

#[test]
fn a_member_and_an_optional_may_have_a_protocol_type() {
    let source = format!(
        "{PROTOCOL_SHAPES}struct Frame {{\n  var shape: Shape\n  var id: Int\n}}\nfunc main() {{\n  let f = Frame(shape: Rect(w: 2, h: 2), id: 1)\n  print(f.shape.area() + f.id)\n  var o: Shape? = Square(side: 5)\n  if let s = o {{ print(s.area()) }}\n  o = nil\n  if o == nil {{ print(0) }}\n}}"
    );
    assert_eq!(run(&source), "5\n25\n0\n");
}

#[test]
fn a_requirement_with_arguments_and_no_result_works_through_a_protocol_value() {
    let source = "protocol Logger {\n  func log(_ code: Int, times: Int)\n}\nstruct Plain: Logger {\n  var base: Int\n  func log(_ code: Int, times: Int) {\n    var i = 0\n    while i < times {\n      print(base + code)\n      i += 1\n    }\n  }\n}\nfunc main() {\n  let l: Logger = Plain(base: 100)\n  l.log(5, times: 2)\n}";
    assert_eq!(run(source), "105\n105\n");
}

#[test]
fn an_overriding_method_of_a_subclass_is_called_through_a_protocol_value() {
    let source = "protocol Named {\n  func code() -> Int\n}\nclass Animal: Named {\n  func code() -> Int { return 1 }\n}\nclass Cat: Animal {\n  override func code() -> Int { return 2 }\n}\nfunc main() {\n  let a: Named = Animal()\n  let c: Named = Cat()\n  print(a.code())\n  print(c.code())\n}";
    assert_eq!(run(source), "1\n2\n");
}

#[test]
fn a_class_held_as_a_protocol_value_is_released_with_its_last_reference() {
    let source = format!(
        "{TRACKED}protocol Idea {{\n  func code() -> Int\n}}\nclass Thought: Tracked, Idea {{\n  func code() -> Int {{ return 7 }}\n}}\nfunc main() {{\n  var i: Idea = Thought(id: 1)\n  print(i.code())\n  i = Thought(id: 2)\n  print(i.code())\n}}"
    );
    assert_eq!(run(&source), "7\n-1\n7\n-2\n");
}

#[test]
fn a_boxed_struct_releases_the_class_it_holds() {
    let source = format!(
        "{TRACKED}protocol Idea {{\n  func code() -> Int\n}}\nstruct Holder: Idea {{\n  var t: Tracked\n  func code() -> Int {{ return t.id }}\n}}\nfunc main() {{\n  let i: Idea = Holder(t: Tracked(id: 5))\n  print(i.code())\n}}"
    );
    assert_eq!(run(&source), "5\n-5\n");
}

#[test]
fn a_protocol_value_can_be_tested_and_cast_to_a_class() {
    let source = format!(
        "{PROTOCOL_SHAPES}func main() {{\n  let a: Shape = Circle(r: 2)\n  let b: Shape = Square(side: 2)\n  print(a is Circle)\n  print(b is Circle)\n  if let c = a as? Circle {{ print(c.r) }}\n}}"
    );
    assert_eq!(run(&source), "true\nfalse\n2\n");
}

#[test]
fn a_type_that_does_not_conform_cannot_become_a_protocol_value() {
    let source = format!(
        "{PROTOCOL_SHAPES}struct Other {{\n  var x: Int\n}}\nfunc main() {{\n  let s: Shape = Other(x: 1)\n}}"
    );
    assert!(build_error(&source).contains("Shape"));
}

#[test]
fn a_protocol_with_an_associated_type_cannot_be_used_as_a_type() {
    let source = format!("{CONTAINER}func f(_ c: Container) {{ }}\nfunc main() {{ }}");
    assert!(build_error(&source).contains("constraint"));
}

#[test]
fn a_protocol_that_mentions_self_cannot_be_used_as_a_type() {
    let source = "protocol Joinable {\n  func join(_ other: Self) -> Int\n}\nfunc f(_ j: Joinable) { }\nfunc main() { }";
    assert!(build_error(source).contains("constraint"));
    let standard = "func f(_ e: Equatable) { }\nfunc main() { }";
    assert!(build_error(standard).contains("constraint"));
}

#[test]
fn a_mutating_requirement_cannot_be_called_through_a_protocol_value() {
    let source = "protocol Counter {\n  mutating func bump()\n}\nstruct Tally: Counter {\n  var n: Int\n  mutating func bump() { n += 1 }\n}\nfunc main() {\n  var c: Counter = Tally(n: 0)\n  c.bump()\n}";
    assert!(build_error(source).contains("mutating"));
}

#[test]
fn a_generic_type_cannot_become_a_protocol_value_yet() {
    let source = "protocol Holder {\n  func get() -> Int\n}\nstruct Box<T>: Holder {\n  var v: T\n  func get() -> Int { return 1 }\n}\nfunc main() {\n  let h: Holder = Box(v: 1)\n}";
    assert!(!build_error(source).is_empty());
}

#[test]
fn a_method_that_is_not_a_requirement_cannot_be_called_on_a_protocol_value() {
    let source = format!(
        "{PROTOCOL_SHAPES}func main() {{\n  let s: Shape = Square(side: 1)\n  print(s.side)\n}}"
    );
    assert!(!build_error(&source).is_empty());
    let method = format!(
        "{PROTOCOL_SHAPES}func main() {{\n  let s: Shape = Square(side: 1)\n  print(s.perimeter())\n}}"
    );
    assert!(build_error(&method).contains("perimeter"));
}

#[test]
fn a_protocol_value_does_not_satisfy_a_constraint_yet() {
    let source = format!(
        "{PROTOCOL_SHAPES}func area_of<T: Shape>(_ s: T) -> Int {{ return s.area() }}\nfunc main() {{\n  let s: Shape = Square(side: 1)\n  print(area_of(s))\n}}"
    );
    assert!(build_error(&source).contains("conform"));
}

#[test]
fn calling_a_requirement_of_a_protocol_nobody_conforms_to_is_rejected() {
    let source = "protocol P {\n  func a() -> Int\n}\nfunc f(_ x: P) -> Int { return x.a() }\nfunc main() { }";
    assert!(build_error(source).contains("conform"));
}

#[test]
fn a_cast_to_a_protocol_is_rejected() {
    let source =
        format!("{PROTOCOL_SHAPES}func main() {{\n  let c = Circle(r: 1)\n  print(c is Shape)\n}}");
    assert!(!build_error(&source).is_empty());
}

fn run_failing(source: &str) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("prog");
    build(source, &exe).unwrap();
    let output = Command::new(&exe).output().unwrap();
    assert!(!output.status.success(), "program should have trapped");
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn fatal_error_writes_its_message_to_standard_error_and_stops_the_program() {
    let (stdout, stderr) =
        run_failing("func main() {\n  print(1)\n  fatalError(\"boom\")\n  print(2)\n}");
    assert_eq!(stdout, "1\n");
    assert_eq!(stderr, "Fatal error: boom\n");
}

#[test]
fn fatal_error_without_a_message_still_reports_itself() {
    let (_, stderr) = run_failing("func main() {\n  fatalError()\n}");
    assert_eq!(stderr, "Fatal error\n");
}

#[test]
fn fatal_error_ends_a_function_that_must_return_a_value() {
    let source = "func positive(_ n: Int) -> Int {\n  if n > 0 { return n }\n  fatalError(\"not positive\")\n}\nfunc main() {\n  print(positive(4))\n}";
    assert_eq!(run(source), "4\n");
    let (_, stderr) = run_failing(&source.replace("positive(4)", "positive(0 - 1)"));
    assert_eq!(stderr, "Fatal error: not positive\n");
}

#[test]
fn fatal_error_can_be_the_way_out_of_a_guard() {
    let source = "func double(_ x: Int?) -> Int {\n  guard let v = x else { fatalError(\"no value\") }\n  return v * 2\n}\nfunc check(_ ok: Bool) {\n  guard ok else { fatalError(\"not ok\") }\n  print(1)\n}\nfunc main() {\n  print(double(4))\n  check(true)\n}";
    assert_eq!(run(source), "8\n1\n");
    let (_, stderr) = run_failing(&source.replace("check(true)", "check(false)"));
    assert_eq!(stderr, "Fatal error: not ok\n");
}

#[test]
fn fatal_error_can_end_every_branch_of_a_switch_and_an_if() {
    let source = "enum E { case a, b }\nfunc pick(_ e: E) -> Int {\n  switch e {\n  case .a: return 1\n  case .b: fatalError(\"b\")\n  }\n}\nfunc sign(_ n: Int) -> Int {\n  if n > 0 { return 1 } else { fatalError(\"no\") }\n}\nfunc main() {\n  print(pick(E.a))\n  print(sign(3))\n}";
    assert_eq!(run(source), "1\n1\n");
}

#[test]
fn fatal_error_in_a_branch_that_is_not_taken_does_nothing() {
    let source = "func main() {\n  var i = 0\n  while i < 3 {\n    if i == 7 { fatalError(\"never\") }\n    i += 1\n  }\n  print(i)\n}";
    assert_eq!(run(source), "3\n");
}

#[test]
fn fatal_error_does_not_run_the_deinit_of_live_objects() {
    let source =
        tracked("func main() {\n  let t = Tracked(id: 1)\n  print(1)\n  fatalError(\"stop\")\n}");
    let (stdout, stderr) = run_failing(&source);
    assert_eq!(stdout, "1\n");
    assert_eq!(stderr, "Fatal error: stop\n");
}

#[test]
fn fatal_error_takes_at_most_one_string_literal() {
    assert!(build_error("func main() {\n  fatalError(1)\n}").contains("fatalError"));
    assert!(build_error("func main() {\n  let s = \"a\"\n  fatalError(s)\n}").contains("literal"));
    assert!(build_error("func main() {\n  fatalError(\"a\", \"b\")\n}").contains("fatalError"));
    assert!(build_error("func main() {\n  fatalError(message: \"a\")\n}").contains("fatalError"));
}

#[test]
fn fatal_error_does_not_produce_a_value() {
    assert!(build_error("func main() {\n  let x = fatalError(\"a\")\n}").contains("fatalError"));
}

#[test]
fn fatal_error_is_a_reserved_function_name() {
    assert!(build_error("func fatalError() { }\nfunc main() { }").contains("fatalError"));
}

#[test]
fn a_long_chain_of_operators_builds_and_runs() {
    let source = format!("func main() {{ print({}) }}", vec!["1"; 7500].join(" + "));
    assert_eq!(run(&source), "7500\n");
}

#[test]
fn a_chain_of_operators_beyond_the_depth_limit_is_an_error_and_not_a_crash() {
    let source = format!("func main() {{ print({}) }}", vec!["1"; 9000].join(" + "));
    assert!(build_error(&source).contains("nesting depth exceeded"));
}

#[test]
fn a_long_else_if_chain_builds_and_runs() {
    let mut source = String::from("func pick(_ n: Int) -> Int {\n  if n == 0 { return 0 }");
    for k in 1..2000 {
        source.push_str(&format!(" else if n == {k} {{ return {k} }}"));
    }
    source.push_str(
        " else { return 0 - 1 }\n}\nfunc main() {\n  print(pick(1999))\n  print(pick(5000))\n}",
    );
    assert_eq!(run(&source), "1999\n-1\n");
}

#[test]
fn a_switch_with_many_cases_builds_and_runs() {
    let mut source = String::from("func pick(_ n: Int) -> Int {\n  switch n {\n");
    for k in 0..2000 {
        source.push_str(&format!("  case {k}: return {}\n", k + 1));
    }
    source.push_str("  default: return 0\n  }\n}\nfunc main() {\n  print(pick(0))\n  print(pick(1999))\n  print(pick(2000))\n}");
    assert_eq!(run(&source), "1\n2000\n0\n");
}

#[test]
fn deeply_nested_parentheses_and_blocks_within_the_limit_build_and_run() {
    let parens = format!(
        "func main() {{ print({}7{}) }}",
        "(".repeat(200),
        ")".repeat(200)
    );
    assert_eq!(run(&parens), "7\n");
    let blocks = format!(
        "func main() {{\n{}print(8)\n{}\n}}",
        "if true {\n".repeat(200),
        "}\n".repeat(200)
    );
    assert_eq!(run(&blocks), "8\n");
}

#[test]
fn nesting_beyond_the_structure_limit_is_an_error_and_not_a_crash() {
    let parens = format!(
        "func main() {{ print({}7{}) }}",
        "(".repeat(400),
        ")".repeat(400)
    );
    assert!(build_error(&parens).contains("structure nesting level exceeded"));
}

#[test]
fn a_long_chain_of_method_calls_builds_and_runs() {
    let source = format!(
        "struct Counter {{\n  var n: Int\n  func next() -> Counter {{ return Counter(n: n + 1) }}\n}}\nfunc main() {{\n  print(Counter(n: 0){}.n)\n}}",
        ".next()".repeat(3000)
    );
    assert_eq!(run(&source), "3000\n");
}

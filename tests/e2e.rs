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
fn a_type_cannot_have_two_initializers_with_the_same_signature() {
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
fn a_bool_is_not_comparable() {
    let source = "func f<T: Comparable>(_ a: T, _ b: T) -> Bool { return a < b }\nfunc main() {\n  print(f(true, false))\n}";
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

#[test]
fn the_remainder_of_the_smallest_integer_by_minus_one_traps() {
    run_expecting_a_crash(
        "func main() {\n  let m = -9223372036854775807 - 1\n  let d = -1\n  print(m % d)\n}",
    );
}

#[test]
fn the_remainder_by_minus_one_is_zero_for_every_other_value() {
    let source = "func main() {\n  let d = -1\n  print(7 % d)\n  print(0 % d)\n  let m = -9223372036854775807 - 1\n  print(m % 2)\n  print((m + 1) % d)\n}";
    assert_eq!(run(source), "0\n0\n0\n0\n");
}

#[test]
fn an_enum_without_payloads_prints_the_name_of_its_case() {
    let source = format!(
        "{LIGHT}func main() {{\n  print(Light.red)\n  var l = Light.green\n  print(l)\n  l.next()\n  print(l)\n}}"
    );
    assert_eq!(run(&source), "red\ngreen\nblue\n");
}

#[test]
fn an_enum_member_and_a_call_result_can_be_printed() {
    let source = "enum Color { case red, green }\nstruct Pixel {\n  var color: Color\n}\nfunc pick(_ n: Int) -> Color {\n  print(100 + n)\n  if n == 0 { return Color.red }\n  return Color.green\n}\nfunc main() {\n  let p = Pixel(color: Color.green)\n  print(p.color)\n  print(pick(0))\n  print(pick(1))\n}";
    assert_eq!(run(source), "green\n100\nred\n101\ngreen\n");
}

#[test]
fn printing_an_enum_evaluates_the_expression_once_and_works_in_a_loop() {
    let source = "enum Color { case red, green }\nfunc next(_ n: Int) -> Color {\n  print(n)\n  if n == 0 { return Color.red }\n  return Color.green\n}\nfunc main() {\n  var i = 0\n  while i < 2 {\n    print(next(i))\n    i += 1\n  }\n}";
    assert_eq!(run(source), "0\nred\n1\ngreen\n");
}

#[test]
fn an_enum_with_payloads_still_cannot_be_printed() {
    let source = "enum Shape { case circle(Int), empty }\nfunc main() {\n  print(Shape.empty)\n}";
    assert!(build_error(source).contains("cannot print"));
}

#[test]
fn print_does_not_break_a_loop_label_inside_an_enum_print() {
    let source = "enum Color { case red }\nfunc main() {\n  outer: for i in 0..<3 {\n    for j in 0..<3 {\n      print(Color.red)\n      if j == 1 { continue outer }\n    }\n  }\n}";
    assert_eq!(run(source), "red\nred\nred\nred\nred\nred\n");
}

#[test]
fn a_generic_function_accepts_explicit_type_arguments() {
    let source = "func id<T>(_ x: T) -> T { return x }\nfunc none<T>() -> T? { return nil }\nfunc main() {\n  print(id<Int>(5))\n  print(id<String>(\"a\"))\n  let x = none<Int>()\n  print(x == nil)\n  let y: Bool? = none<Bool>()\n  print(y == nil)\n}";
    assert_eq!(run(source), "5\na\ntrue\ntrue\n");
}

#[test]
fn explicit_type_arguments_are_checked_against_the_arguments() {
    let source = "func id<T>(_ x: T) -> T { return x }\nfunc main() {\n  print(id<Int>(\"a\"))\n}";
    assert!(build_error(source).contains("expected Int"));
}

#[test]
fn the_number_of_explicit_type_arguments_must_match() {
    let two = "func id<T>(_ x: T) -> T { return x }\nfunc main() {\n  print(id<Int, Bool>(1))\n}";
    assert!(build_error(two).contains("expects 1 type argument"));
    let none = "func plain() -> Int { return 1 }\nfunc main() {\n  print(plain<Int>())\n}";
    assert!(build_error(none).contains("expects 0 type argument"));
}

#[test]
fn explicit_type_arguments_must_satisfy_the_constraints() {
    let source = "protocol Shape {\n  func area() -> Int\n}\nstruct Other {\n  var x: Int\n}\nfunc area_of<T: Shape>(_ s: T) -> Int { return s.area() }\nfunc main() {\n  print(area_of<Other>(Other(x: 1)))\n}";
    let message = build_error(source);
    assert!(
        message.contains("Other") && message.contains("Shape"),
        "{message}"
    );
}

#[test]
fn a_generic_method_accepts_explicit_type_arguments() {
    let source = "struct Box<T> {\n  var v: T\n  func pick<U>(_ x: U) -> U { return x }\n  func none<U>() -> U? { return nil }\n  func note<U>(_ x: U) { print(1) }\n}\nfunc main() {\n  let b = Box(v: 7)\n  print(b.pick<Bool>(true))\n  let n = b.none<Int>()\n  print(n == nil)\n  b.note<String>(\"s\")\n}";
    assert_eq!(run(source), "true\ntrue\n1\n");
}

#[test]
fn explicit_type_arguments_work_on_a_method_of_a_plain_type_and_on_self() {
    let source = "struct Tool {\n  var k: Int\n  func twice<U>(_ x: U) -> U { return x }\n  func call() -> Int {\n    let a = self.twice<Int>(k)\n    let b = twice<Int>(a)\n    return b + 1\n  }\n}\nfunc main() {\n  let t = Tool(k: 4)\n  print(t.twice<Int>(2))\n  print(t.call())\n}";
    assert_eq!(run(source), "2\n5\n");
}

#[test]
fn explicit_method_type_arguments_are_counted_without_those_of_the_type() {
    let wrong = "struct Box<T> {\n  var v: T\n  func pick<U>(_ x: U) -> U { return x }\n}\nfunc main() {\n  let b = Box(v: 1)\n  print(b.pick<Int, Bool>(1))\n}";
    assert!(build_error(wrong).contains("expects 1 type argument"));
    let none = "struct Tool {\n  var k: Int\n  func get() -> Int { return k }\n}\nfunc main() {\n  let t = Tool(k: 1)\n  print(t.get<Int>())\n}";
    assert!(build_error(none).contains("expects 0 type argument"));
}

#[test]
fn explicit_type_arguments_cannot_be_given_to_a_requirement() {
    let source = "protocol P {\n  func a() -> Int\n}\nfunc f<T: P>(_ x: T) -> Int { return x.a<Int>() }\nfunc main() { }";
    assert!(!build_error(source).is_empty());
}

const NESTED: &str = "struct Inner {\n  var n: Int\n  var m: Int\n}\nstruct Outer {\n  var head: Int\n  var inner: Inner\n  var tail: Int\n}\n";

#[test]
fn a_member_of_a_struct_member_can_be_assigned() {
    let source = format!(
        "{NESTED}func main() {{\n  var o = Outer(head: 1, inner: Inner(n: 2, m: 3), tail: 4)\n  o.inner.n = 20\n  print(o.head)\n  print(o.inner.n)\n  print(o.inner.m)\n  print(o.tail)\n}}"
    );
    assert_eq!(run(&source), "1\n20\n3\n4\n");
}

#[test]
fn a_nested_member_supports_compound_assignment() {
    let source = format!(
        "{NESTED}func main() {{\n  var o = Outer(head: 1, inner: Inner(n: 2, m: 3), tail: 4)\n  o.inner.m += 10\n  o.inner.m *= 2\n  print(o.inner.m)\n  print(o.inner.n)\n}}"
    );
    assert_eq!(run(&source), "26\n2\n");
}

#[test]
fn a_member_can_be_assigned_three_levels_deep() {
    let source = "struct C { var v: Int }\nstruct B { var c: C\n var w: Int }\nstruct A { var b: B\n var x: Int }\nfunc main() {\n  var a = A(b: B(c: C(v: 1), w: 2), x: 3)\n  a.b.c.v = 7\n  print(a.b.c.v)\n  print(a.b.w)\n  print(a.x)\n}";
    assert_eq!(run(source), "7\n2\n3\n");
}

#[test]
fn a_nested_member_of_a_class_instance_can_be_assigned_through_a_constant() {
    let source = "struct Point {\n  var x: Int\n  var y: Int\n}\nclass Shape {\n  var origin: Point\n  var id: Int\n}\nfunc main() {\n  let s = Shape(origin: Point(x: 1, y: 2), id: 5)\n  s.origin.x = 10\n  s.origin.y += 1\n  print(s.origin.x)\n  print(s.origin.y)\n  print(s.id)\n}";
    assert_eq!(run(source), "10\n3\n5\n");
}

#[test]
fn a_member_of_an_object_reached_through_a_constant_struct_can_be_assigned() {
    let source = "class Node {\n  var v: Int\n}\nstruct Holder {\n  let node: Node\n}\nfunc main() {\n  let h = Holder(node: Node(v: 1))\n  h.node.v = 9\n  print(h.node.v)\n}";
    assert_eq!(run(source), "9\n");
}

#[test]
fn a_member_of_an_object_reached_through_another_object_can_be_assigned() {
    let source = "class Leaf {\n  var v: Int\n}\nclass Branch {\n  var leaf: Leaf\n}\nfunc main() {\n  let b = Branch(leaf: Leaf(v: 1))\n  b.leaf.v = 4\n  b.leaf.v += 1\n  print(b.leaf.v)\n}";
    assert_eq!(run(source), "5\n");
}

#[test]
fn nested_members_can_be_assigned_through_the_implicit_self() {
    let source = "struct Point {\n  var x: Int\n  var y: Int\n}\nclass Shape {\n  var origin: Point\n  func move() {\n    self.origin.x = 3\n    origin.y = 4\n  }\n}\nstruct Pair {\n  var p: Point\n  mutating func reset() {\n    p.x = 0\n    self.p.y = 0\n  }\n}\nfunc main() {\n  let s = Shape(origin: Point(x: 1, y: 2))\n  s.move()\n  print(s.origin.x + s.origin.y)\n  var pair = Pair(p: Point(x: 5, y: 6))\n  pair.reset()\n  print(pair.p.x + pair.p.y)\n}";
    assert_eq!(run(source), "7\n0\n");
}

#[test]
fn an_initializer_may_assign_a_nested_member_once_every_member_is_set() {
    let source = "struct Inner {\n  var n: Int\n}\nstruct Outer {\n  var inner: Inner\n  var k: Int\n  init(n: Int) {\n    self.inner = Inner(n: 0)\n    self.k = 1\n    self.inner.n = n\n  }\n}\nfunc main() {\n  print(Outer(n: 8).inner.n)\n}";
    assert_eq!(run(source), "8\n");
}

#[test]
fn assigning_a_nested_class_member_releases_the_old_instance() {
    let source = tracked(
        "struct Holder {\n  var t: Tracked\n}\nclass Box {\n  var h: Holder\n}\nfunc main() {\n  let b = Box(h: Holder(t: Tracked(id: 1)))\n  b.h.t = Tracked(id: 2)\n  print(b.h.t.id)\n}",
    );
    assert_eq!(run(&source), "-1\n2\n-2\n");
}

#[test]
fn a_nested_weak_member_can_be_assigned() {
    let source = "class Parent {\n  var id: Int\n}\nclass Child {\n  weak var parent: Parent?\n  var id: Int\n}\nstruct Holder {\n  var child: Child\n}\nfunc main() {\n  let p = Parent(id: 3)\n  let h = Holder(child: Child(parent: nil, id: 1))\n  h.child.parent = p\n  if let q = h.child.parent { print(q.id) }\n}";
    assert_eq!(run(source), "3\n");
}

#[test]
fn a_nested_member_cannot_be_assigned_through_a_constant_value() {
    let source = format!(
        "{NESTED}func main() {{\n  let o = Outer(head: 1, inner: Inner(n: 2, m: 3), tail: 4)\n  o.inner.n = 5\n}}"
    );
    assert!(build_error(&source).contains("constant"));
}

#[test]
fn a_nested_member_cannot_be_assigned_through_a_let_member() {
    let source = "struct Inner {\n  var n: Int\n}\nstruct Outer {\n  let inner: Inner\n}\nfunc main() {\n  var o = Outer(inner: Inner(n: 1))\n  o.inner.n = 2\n}";
    assert!(build_error(source).contains("constant"));
    let last = "struct Inner {\n  let n: Int\n}\nstruct Outer {\n  var inner: Inner\n}\nfunc main() {\n  var o = Outer(inner: Inner(n: 1))\n  o.inner.n = 2\n}";
    assert!(build_error(last).contains("constant"));
}

#[test]
fn only_a_member_path_rooted_in_a_variable_can_be_assigned() {
    let source = "struct Inner {\n  var n: Int\n}\nfunc make() -> Inner { return Inner(n: 1) }\nfunc main() {\n  make().n = 2\n}";
    assert!(build_error(source).contains("only a member of a variable"));
}

#[test]
fn a_nested_assignment_checks_the_type_of_the_member() {
    let source = format!(
        "{NESTED}func main() {{\n  var o = Outer(head: 1, inner: Inner(n: 2, m: 3), tail: 4)\n  o.inner.n = true\n}}"
    );
    assert!(build_error(&source).contains("expected Int"));
}

#[test]
fn a_member_path_through_an_unowned_member_cannot_be_assigned() {
    let source = "class Leaf {\n  var v: Int\n}\nclass Branch {\n  unowned var leaf: Leaf\n}\nfunc main() {\n  let l = Leaf(v: 1)\n  let b = Branch(leaf: l)\n  b.leaf.v = 2\n}";
    assert!(build_error(source).contains("unowned"));
}

#[test]
fn initializers_with_different_argument_labels_can_coexist() {
    let source = "struct P {\n  var x: Int\n  var y: Int\n  init(x: Int) {\n    self.x = x\n    self.y = 0\n  }\n  init(y: Int) {\n    self.x = 0\n    self.y = y\n  }\n  init() {\n    self.x = 7\n    self.y = 7\n  }\n}\nfunc main() {\n  let a = P(x: 1)\n  let b = P(y: 2)\n  let c = P()\n  print(a.x * 100 + a.y)\n  print(b.x * 100 + b.y)\n  print(c.x * 100 + c.y)\n}";
    assert_eq!(run(source), "100\n2\n707\n");
}

#[test]
fn initializers_are_chosen_by_the_type_of_the_argument() {
    let source = "struct V {\n  var tag: Int\n  init(_ n: Int) { self.tag = 1 }\n  init(_ s: String) { self.tag = 2 }\n  init(_ b: Bool) { self.tag = 3 }\n}\nfunc main() {\n  print(V(5).tag)\n  print(V(\"a\").tag)\n  print(V(true).tag)\n}";
    assert_eq!(run(source), "1\n2\n3\n");
}

#[test]
fn initializers_are_chosen_by_the_number_of_arguments() {
    let source = "struct R {\n  var area: Int\n  init(side: Int) { self.area = side * side }\n  init(w: Int, h: Int) { self.area = w * h }\n}\nfunc main() {\n  print(R(side: 3).area)\n  print(R(w: 2, h: 5).area)\n}";
    assert_eq!(run(source), "9\n10\n");
}

#[test]
fn the_most_specific_class_initializer_wins() {
    let source = "class Animal {\n  var id: Int\n}\nclass Cat: Animal {\n  var lives: Int\n}\nstruct Kennel {\n  var kind: Int\n  init(_ a: Animal) { self.kind = 1 }\n  init(_ c: Cat) { self.kind = 2 }\n}\nfunc main() {\n  let cat = Cat(id: 1, lives: 9)\n  let animal = Animal(id: 2)\n  print(Kennel(cat).kind)\n  print(Kennel(animal).kind)\n}";
    assert_eq!(run(source), "2\n1\n");
}

#[test]
fn an_exact_match_beats_a_conversion() {
    let source = "struct W {\n  var kind: Int\n  init(_ n: Int?) { self.kind = 1 }\n  init(_ n: Int) { self.kind = 2 }\n  init(_ s: String) { self.kind = 3 }\n}\nfunc main() {\n  print(W(4).kind)\n  let o: Int? = 4\n  print(W(o).kind)\n  print(W(\"x\").kind)\n}";
    assert_eq!(run(source), "2\n1\n3\n");
}

#[test]
fn a_class_and_an_enum_may_have_several_initializers() {
    let source = "class Counter {\n  var n: Int\n  init() { self.n = 0 }\n  init(start: Int) { self.n = start }\n}\nenum Level {\n  case low, high\n  init(code: Int) {\n    if code > 5 { self = Level.high } else { self = Level.low }\n  }\n  init(flag: Bool) {\n    if flag { self = Level.high } else { self = Level.low }\n  }\n}\nfunc main() {\n  print(Counter().n)\n  print(Counter(start: 4).n)\n  print(Level(code: 9) == Level.high)\n  print(Level(flag: false) == Level.low)\n}";
    assert_eq!(run(source), "0\n4\ntrue\ntrue\n");
}

#[test]
fn a_subclass_chooses_the_superclass_initializer_by_its_arguments() {
    let source = "class Base {\n  var code: Int\n  init(code: Int) { self.code = code }\n  init(name: String) { self.code = 99 }\n}\nclass Child: Base {\n  var extra: Int\n  init(extra: Int) {\n    self.extra = extra\n    super.init(code: extra + 1)\n  }\n  init(label: String) {\n    self.extra = 0\n    super.init(name: label)\n  }\n}\nfunc main() {\n  let a = Child(extra: 4)\n  let b = Child(label: \"x\")\n  print(a.code)\n  print(b.code)\n}";
    assert_eq!(run(source), "5\n99\n");
}

#[test]
fn a_generic_type_may_have_several_initializers() {
    let source = "struct Pair<T> {\n  var a: T\n  var b: T\n  init(value: T) {\n    self.a = value\n    self.b = value\n  }\n  init(first: T, second: T) {\n    self.a = first\n    self.b = second\n  }\n}\nfunc main() {\n  let p = Pair(value: 3)\n  let q = Pair(first: 1, second: 2)\n  let r = Pair<Int>(value: 9)\n  print(p.a + p.b)\n  print(q.a * 10 + q.b)\n  print(r.a)\n}";
    assert_eq!(run(source), "6\n12\n9\n");
}

#[test]
fn two_initializers_with_the_same_labels_and_types_are_rejected() {
    let source = "struct S {\n  var x: Int\n  init(x: Int) { self.x = x }\n  init(x: Int) { self.x = 0 }\n}\nfunc main() { }";
    assert!(build_error(source).contains("duplicate"));
}

#[test]
fn a_call_that_matches_no_initializer_lists_the_choices() {
    let source = "struct S {\n  var x: Int\n  init(x: Int) { self.x = x }\n  init(y: Int) { self.x = y }\n}\nfunc main() {\n  let s = S(z: 1)\n}";
    let message = build_error(source);
    assert!(message.contains("no initializer of 'S'"), "{message}");
    let by_type = "struct S {\n  var x: Int\n  init(_ x: Int) { self.x = x }\n  init(_ s: String) { self.x = 0 }\n}\nfunc main() {\n  let s = S(true)\n}";
    assert!(build_error(by_type).contains("no initializer of 'S'"));
}

#[test]
fn an_ambiguous_initializer_call_is_rejected() {
    let source = "struct S {\n  var x: Int\n  init(_ a: Int?) { self.x = 1 }\n  init(_ b: Bool?) { self.x = 2 }\n}\nfunc main() {\n  let s = S(nil)\n}";
    assert!(!build_error(source).is_empty());
}

#[test]
fn a_type_with_one_initializer_keeps_its_original_diagnostics() {
    let source = "struct S {\n  var x: Int\n  init(x: Int) { self.x = x }\n}\nfunc main() {\n  let s = S(1, 2)\n}";
    assert!(build_error(source).contains("takes 1 argument"));
}

#[test]
fn an_initializer_may_delegate_to_another_one() {
    let source = "struct P {\n  var x: Int\n  var y: Int\n  init(x: Int, y: Int) {\n    self.x = x\n    self.y = y\n  }\n  init(side: Int) {\n    self.init(x: side, y: side)\n  }\n  init() {\n    self.init(side: 5)\n    self.y += 1\n  }\n}\nfunc main() {\n  let a = P(side: 3)\n  let b = P()\n  print(a.x + a.y)\n  print(b.x * 10 + b.y)\n}";
    assert_eq!(run(source), "6\n56\n");
}

#[test]
fn a_delegating_initializer_may_use_its_parameters_and_self_afterwards() {
    let source = "struct Range {\n  var lo: Int\n  var hi: Int\n  init(lo: Int, hi: Int) {\n    self.lo = lo\n    self.hi = hi\n  }\n  init(size: Int) {\n    let doubled = size * 2\n    self.init(lo: 0, hi: doubled)\n    if self.hi > 10 { self.hi = 10 }\n    print(self.hi)\n  }\n}\nfunc main() {\n  let r = Range(size: 8)\n  print(r.hi)\n  let s = Range(size: 2)\n  print(s.hi)\n}";
    assert_eq!(run(source), "10\n10\n4\n4\n");
}

#[test]
fn a_class_initializer_may_delegate_without_running_deinit_twice() {
    let source = "class Tracked {\n  var id: Int\n  var tag: Int\n  init(id: Int, tag: Int) {\n    self.id = id\n    self.tag = tag\n  }\n  init(id: Int) {\n    self.init(id: id, tag: 7)\n  }\n  deinit { print(0 - self.id) }\n}\nfunc main() {\n  let t = Tracked(id: 3)\n  print(t.tag)\n}";
    assert_eq!(run(source), "7\n-3\n");
}

#[test]
fn an_enum_initializer_may_delegate_on_every_path() {
    let source = "enum Level {\n  case low, high\n  init(code: Int) {\n    if code > 5 { self = Level.high } else { self = Level.low }\n  }\n  init(flag: Bool) {\n    if flag { self.init(code: 9) } else { self.init(code: 0) }\n  }\n  init() {\n    self.init(flag: true)\n  }\n}\nfunc main() {\n  print(Level(flag: true) == Level.high)\n  print(Level(flag: false) == Level.low)\n  print(Level() == Level.high)\n}";
    assert_eq!(run(source), "true\ntrue\ntrue\n");
}

#[test]
fn a_generic_initializer_may_delegate() {
    let source = "struct Box<T> {\n  var v: T\n  var w: T\n  init(v: T, w: T) {\n    self.v = v\n    self.w = w\n  }\n  init(both: T) {\n    self.init(v: both, w: both)\n  }\n}\nfunc main() {\n  let b = Box(both: 4)\n  print(b.v + b.w)\n}";
    assert_eq!(run(source), "8\n");
}

#[test]
fn a_subclass_initializer_may_delegate_to_one_that_calls_super() {
    let source = "class Base {\n  var code: Int\n  init(code: Int) { self.code = code }\n}\nclass Child: Base {\n  var extra: Int\n  init(extra: Int, code: Int) {\n    self.extra = extra\n    super.init(code: code)\n  }\n  init(extra: Int) {\n    self.init(extra: extra, code: 100)\n  }\n}\nfunc main() {\n  let c = Child(extra: 4)\n  print(c.extra + c.code)\n}";
    assert_eq!(run(source), "104\n");
}

#[test]
fn a_delegating_initializer_cannot_use_self_before_it_delegates() {
    let before = "struct P {\n  var x: Int\n  init(x: Int) { self.x = x }\n  init() {\n    self.x = 1\n    self.init(x: 2)\n  }\n}\nfunc main() { }";
    assert!(build_error(before).contains("before"));
    let args = "struct P {\n  var x: Int\n  init(x: Int) { self.x = x }\n  init() {\n    self.init(x: self.x)\n  }\n}\nfunc main() { }";
    assert!(!build_error(args).is_empty());
}

#[test]
fn delegation_must_be_a_top_level_statement_called_once() {
    let nested = "struct P {\n  var x: Int\n  init(x: Int) { self.x = x }\n  init(flag: Bool) {\n    if flag { self.init(x: 1) } else { self.x = 2 }\n  }\n}\nfunc main() { }";
    assert!(build_error(nested).contains("top-level"));
    let twice = "struct P {\n  var x: Int\n  init(x: Int) { self.x = x }\n  init() {\n    self.init(x: 1)\n    self.init(x: 2)\n  }\n}\nfunc main() { }";
    assert!(build_error(twice).contains("twice"));
}

#[test]
fn an_initializer_cannot_delegate_to_itself() {
    let source =
        "struct P {\n  var x: Int\n  init(x: Int) {\n    self.init(x: x)\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("itself"));
}

#[test]
fn delegation_is_possible_only_inside_an_initializer() {
    let source = "struct P {\n  var x: Int\n  init(x: Int) { self.x = x }\n  func again() {\n    self.init(x: 1)\n  }\n}\nfunc main() { }";
    assert!(!build_error(source).is_empty());
}

#[test]
fn an_initializer_that_does_not_delegate_still_initializes_every_member() {
    let source = "struct P {\n  var x: Int\n  var y: Int\n  init(x: Int, y: Int) {\n    self.x = x\n    self.y = y\n  }\n  init(x: Int) {\n    self.x = x\n  }\n}\nfunc main() { }";
    assert!(build_error(source).contains("'y'"));
}

#[test]
fn a_payload_enum_may_carry_a_string_and_another_type_at_the_same_position() {
    let source = "enum Value {\n  case number(Int)\n  case text(String)\n  case flag(Bool)\n  case pair(String, Int)\n}\nfunc show(_ v: Value) {\n  switch v {\n  case .number(let n): print(n)\n  case .text(let s): print(s)\n  case .flag(let b): print(b)\n  case .pair(let s, let n):\n    print(s)\n    print(n)\n  }\n}\nfunc main() {\n  show(Value.number(7))\n  show(Value.text(\"hi\"))\n  show(Value.flag(true))\n  show(Value.pair(\"k\", 3))\n}";
    assert_eq!(run(source), "7\nhi\ntrue\nk\n3\n");
}

#[test]
fn a_string_can_be_stored_copied_and_returned_through_every_kind_of_owner() {
    let source = "struct Label {\n  var text: String\n  var n: Int\n}\nclass Holder {\n  var label: Label\n  var name: String\n}\nfunc pick(_ flag: Bool) -> String {\n  let a = \"first\"\n  let b = \"second\"\n  if flag { return a }\n  return b\n}\nfunc main() {\n  var l = Label(text: \"x\", n: 1)\n  let copy = l\n  l.text = \"y\"\n  let h = Holder(label: copy, name: pick(false))\n  var o: String? = pick(true)\n  print(l.text)\n  print(copy.text)\n  print(h.label.text)\n  print(h.name)\n  if let s = o { print(s) }\n  o = nil\n  if o == nil { print(0) }\n}";
    assert_eq!(run(source), "y\nx\nx\nsecond\nfirst\n0\n");
}

#[test]
fn strings_can_be_concatenated_with_plus() {
    let source = "func main() {\n  let a = \"foo\"\n  let b = \"bar\"\n  let c = a + b\n  print(c)\n  print(a + \"-\" + b + \"!\")\n  print(\"\" + \"x\")\n  print(\"x\" + \"\")\n  print(\"\" + \"\")\n}";
    assert_eq!(run(source), "foobar\nfoo-bar!\nx\nx\n\n");
}

#[test]
fn a_string_can_be_extended_with_plus_equals() {
    let source = "func main() {\n  var s = \"a\"\n  s += \"b\"\n  s += \"cd\"\n  print(s)\n  s += s\n  print(s)\n  var t = \"\"\n  var i = 0\n  while i < 5 {\n    t += \"x\"\n    i += 1\n  }\n  print(t)\n}";
    assert_eq!(run(source), "abcd\nabcdabcd\nxxxxx\n");
}

#[test]
fn count_and_is_empty_describe_the_bytes_of_a_string() {
    let source = "func main() {\n  let s = \"hello\"\n  print(s.count)\n  print(s.isEmpty)\n  print(\"\".count)\n  print(\"\".isEmpty)\n  print((s + \" world\").count)\n  print((\"\" + \"\").isEmpty)\n  print(\"é\".count)\n}";
    assert_eq!(run(source), "5\nfalse\n0\ntrue\n11\ntrue\n2\n");
}

#[test]
fn a_concatenated_string_can_be_compared_switched_on_and_passed_around() {
    let source = "func twice(_ s: String) -> String { return s + s }\nfunc kind(_ s: String) -> Int {\n  switch s {\n  case \"ab\": return 1\n  case \"abab\": return 2\n  default: return 0\n  }\n}\nfunc main() {\n  let ab = \"a\" + \"b\"\n  print(ab == \"ab\")\n  print(kind(ab))\n  print(kind(twice(ab)))\n  print(kind(twice(twice(\"x\"))))\n  if twice(ab) == \"abab\" { print(1) }\n}";
    assert_eq!(run(source), "true\n1\n2\n0\n1\n");
}

#[test]
fn a_concatenated_string_can_be_stored_in_every_kind_of_owner() {
    let source = "struct Label {\n  var text: String\n}\nclass Holder {\n  var label: Label\n  var name: String\n}\nfunc build(_ n: Int) -> String {\n  var s = \"n\"\n  var i = 0\n  while i < n {\n    s += \"!\"\n    i += 1\n  }\n  return s\n}\nfunc main() {\n  let h = Holder(label: Label(text: build(1)), name: build(2))\n  h.name = h.name + h.label.text\n  h.label.text += \"?\"\n  var o: String? = build(3)\n  print(h.name)\n  print(h.label.text)\n  if let s = o { print(s + \".\") }\n  o = nil\n  if o == nil { print(0) }\n}";
    assert_eq!(run(source), "n!!n!\nn!?\nn!!!.\n0\n");
}

#[test]
fn many_concatenations_in_a_loop_do_not_crash() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    let s = \"item\" + \"-\" + \"x\"\n    total += s.count\n    i += 1\n  }\n  print(total)\n  var big = \"\"\n  var j = 0\n  while j < 1000 {\n    big += \"ab\"\n    j += 1\n  }\n  print(big.count)\n}";
    assert_eq!(run(source), "120000\n2000\n");
}

#[test]
fn a_string_cannot_be_combined_with_other_types_or_operators() {
    assert!(build_error("func main() { print(\"a\" + 1) }").contains("expected String"));
    assert!(build_error("func main() { print(1 + \"a\") }").contains("expected Int"));
    assert!(build_error("func main() { print(\"a\" - \"b\") }").contains("expected Int"));
    assert!(
        build_error("func main() {\n  var s = \"a\"\n  s -= \"b\"\n}").contains("expected Int")
    );
    assert!(build_error("func main() {\n  var s = \"a\"\n  s += 1\n}").contains("expected String"));
}

#[test]
fn a_string_has_only_the_members_count_and_is_empty() {
    assert!(build_error("func main() { print(\"a\".length) }").contains("length"));
    assert!(!build_error("func main() {\n  let s = \"a\"\n  s.count = 1\n}").is_empty());
}

#[test]
fn strings_are_ordered_by_their_bytes() {
    let source = "func main() {\n  print(\"a\" < \"b\")\n  print(\"b\" < \"a\")\n  print(\"a\" < \"ab\")\n  print(\"abc\" > \"ab\")\n  print(\"b\" > \"abc\")\n  print(\"ab\" <= \"ab\")\n  print(\"ab\" >= \"ab\")\n  print(\"ab\" < \"ab\")\n  print(\"\" < \"a\")\n  print(\"a\" <= \"\")\n  print(\"z\" < \"é\")\n  let x = \"ap\" + \"ple\"\n  print(x < \"pear\")\n}";
    assert_eq!(
        run(source),
        "true\nfalse\ntrue\ntrue\ntrue\ntrue\ntrue\nfalse\ntrue\nfalse\ntrue\ntrue\n"
    );
}

#[test]
fn a_string_is_comparable_through_a_constraint() {
    let source = "func largest<T: Comparable>(_ a: T, _ b: T) -> T {\n  if a > b { return a }\n  return b\n}\nfunc main() {\n  print(largest(\"pear\", \"apple\"))\n  print(largest(\"a\" + \"z\", \"b\"))\n  print(largest(3, 9))\n}";
    assert_eq!(run(source), "pear\nb\n9\n");
}

#[test]
fn ordering_a_string_with_another_type_is_rejected() {
    assert!(build_error("func main() { print(\"a\" < 1) }").contains("expected String"));
    assert!(build_error("func main() { print(1 < \"a\") }").contains("expected Int"));
}

#[test]
fn a_string_can_be_made_from_an_integer_a_boolean_or_a_string() {
    let source = "func main() {\n  print(String(42))\n  print(String(0))\n  print(String(-7))\n  print(String(9223372036854775807))\n  print(String(-9223372036854775807 - 1))\n  print(String(true))\n  print(String(false))\n  print(String(\"same\"))\n  print(String(1 + 2) + \"!\")\n  print(\"n=\" + String(5))\n  print(String(12345).count)\n}";
    assert_eq!(
        run(source),
        "42\n0\n-7\n9223372036854775807\n-9223372036854775808\ntrue\nfalse\nsame\n3!\nn=5\n5\n"
    );
}

#[test]
fn a_string_can_be_made_from_an_enum_without_payloads() {
    let source = "enum Color { case red, green, blue }\nfunc name(_ c: Color) -> String { return String(c) }\nfunc main() {\n  print(String(Color.green))\n  print(name(Color.blue) + \"/\" + name(Color.red))\n}";
    assert_eq!(run(source), "green\nblue/red\n");
}

#[test]
fn converting_strings_in_a_loop_does_not_leak() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    total += String(i).count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "88890\n");
}

#[test]
fn a_string_can_be_tested_for_a_prefix_a_suffix_and_a_substring() {
    let source = "func main() {\n  let s = \"hello world\"\n  print(s.hasPrefix(\"hello\"))\n  print(s.hasPrefix(\"world\"))\n  print(s.hasSuffix(\"world\"))\n  print(s.hasSuffix(\"hello\"))\n  print(s.contains(\"o w\"))\n  print(s.contains(\"xyz\"))\n  print(s.hasPrefix(\"\"))\n  print(s.hasSuffix(\"\"))\n  print(s.contains(\"\"))\n  print(s.hasPrefix(s))\n  print(s.hasPrefix(s + \"!\"))\n  print(s.hasSuffix(s + \"!\"))\n  print(s.contains(s + \"!\"))\n  print(\"\".hasPrefix(\"a\"))\n  print(\"\".contains(\"\"))\n  print((\"ab\" + \"cd\").hasSuffix(\"cd\"))\n  print(\"é!\".hasPrefix(\"é\"))\n  print(\"abc\".contains(\"c\"))\n}";
    assert_eq!(
        run(source),
        "true\nfalse\ntrue\nfalse\ntrue\nfalse\ntrue\ntrue\ntrue\ntrue\nfalse\nfalse\nfalse\nfalse\ntrue\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn string_conversions_and_searches_are_rejected_when_misused() {
    assert!(
        build_error("struct P { var x: Int }\nfunc main() { print(String(P(x: 1))) }")
            .contains("cannot convert")
    );
    assert!(build_error("func main() { print(String()) }").contains("String"));
    assert!(build_error("func main() { print(String(1, 2)) }").contains("String"));
    assert!(build_error("func main() { print(String(value: 1)) }").contains("String"));
    assert!(build_error("func main() { print(\"a\".hasPrefix(1)) }").contains("expected String"));
    assert!(build_error("func main() { print(\"a\".hasPrefix()) }").contains("hasPrefix"));
    assert!(build_error("func main() { print(\"a\".shout()) }").contains("shout"));
}

#[test]
fn a_string_literal_may_embed_expressions() {
    let source = "enum Color { case red, green }\nstruct P {\n  var name: String\n}\nfunc twice(_ n: Int) -> Int { return n * 2 }\nfunc main() {\n  let a = 4\n  let s = \"hi\"\n  print(\"x=\\(1 + 2)\")\n  print(\"\\(a) and \\(s)!\")\n  print(\"\\(true)/\\(Color.green)\")\n  print(\"twice: \\(twice(a))\")\n  print(\"len \\(s.count)\")\n  print(\"\\(a)\")\n  print(\"\\(a)\\(a)\")\n  print(\"[\\(P(name: \"q\").name)]\")\n  print(\"\\(-a)\")\n}";
    assert_eq!(
        run(source),
        "x=3\n4 and hi!\ntrue/green\ntwice: 8\nlen 2\n4\n44\n[q]\n-4\n"
    );
}

#[test]
fn an_embedded_expression_may_contain_strings_and_interpolations() {
    let source = "func main() {\n  let n = 7\n  print(\"a\\(\"in\" + \"ner\")b\")\n  print(\"a\\(\"b\\(n)c\")d\")\n  print(\"(\\(n))\")\n  print(\"\\(\"\\(\"x\")\")\")\n}";
    assert_eq!(run(source), "ainnerb\nab7cd\n(7)\nx\n");
}

#[test]
fn an_escaped_backslash_does_not_start_an_interpolation() {
    let source =
        "func main() {\n  print(\"\\\\(1)\")\n  print(\"a\\nb\")\n  print(\"q\\\"\\(1)\\\"\")\n}";
    assert_eq!(run(source), "\\(1)\na\nb\nq\"1\"\n");
}

#[test]
fn an_interpolated_string_is_an_ordinary_string() {
    let source = "func label(_ n: Int) -> String { return \"#\\(n)\" }\nfunc main() {\n  var s = \"\"\n  var i = 0\n  while i < 3 {\n    s += label(i)\n    i += 1\n  }\n  print(s)\n  print(s.count)\n  switch label(2) {\n  case \"#2\": print(1)\n  default: print(0)\n  }\n  print(label(5) == \"#5\")\n  let t: String? = \"o\\(1)\"\n  if let u = t { print(u) }\n}";
    assert_eq!(run(source), "#0#1#2\n6\n1\ntrue\no1\n");
}

#[test]
fn interpolating_in_a_loop_does_not_leak() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    let s = \"item \\(i) of \\(i + 1)\"\n    total += s.count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "357784\n");
}

#[test]
fn a_value_that_cannot_become_a_string_cannot_be_interpolated() {
    let source = "struct P { var x: Int }\nfunc main() {\n  print(\"\\(P(x: 1))\")\n}";
    assert!(build_error(source).contains("cannot convert"));
}

#[test]
fn a_malformed_interpolation_is_an_error() {
    assert!(!build_error("func main() { print(\"a\\()\") }").is_empty());
    assert!(build_error("func main() { print(\"a\\(1\") }").contains("interpolation"));
    assert!(!build_error("func main() { print(\"a\\(1 +)\") }").is_empty());
}

const SHAPES_ENUM: &str = "enum Color { case red, green }\nenum Shape {\n  case circle(Int)\n  case rect(Int, Int)\n  case tinted(Color)\n  case empty\n}\nfunc area(_ s: Shape) -> Int {\n  switch s {\n  case .circle(let r): return 3 * r * r\n  case .rect(let w, let h): return w * h\n  case .tinted(let c):\n    if c == Color.red { return 1 }\n    return 2\n  case .empty: return 0\n  }\n}\n";

#[test]
fn an_enum_case_may_omit_the_type_name_where_the_type_is_known() {
    let source = "enum Color { case red, green }\nenum Shape {\n  case circle(Int)\n  case rect(Int, Int)\n  case empty\n}\nfunc area(_ s: Shape) -> Int {\n  switch s {\n  case .circle(let r): return 3 * r * r\n  case .rect(let w, let h): return w * h\n  case .empty: return 0\n  }\n}\nfunc make(_ n: Int) -> Shape {\n  if n == 0 { return .empty }\n  return .circle(n)\n}\nfunc main() {\n  let a: Shape = .circle(2)\n  let b: Shape = .rect(3, 4)\n  var c: Shape = .empty\n  print(area(a))\n  print(area(b))\n  print(area(c))\n  c = .rect(1, 5)\n  print(area(c))\n  print(area(.circle(1)))\n  print(area(make(0)))\n  print(area(make(3)))\n  let g: Color = .green\n  print(g == Color.green)\n}";
    assert_eq!(run(source), "12\n12\n0\n5\n3\n0\n27\ntrue\n");
}

#[test]
fn an_implicit_case_works_as_a_member_value_a_payload_and_an_operand() {
    let source = "enum Color { case red, green }\nenum Mark {\n  case tint(Color)\n  case none\n}\nstruct Pixel {\n  var color: Color\n  var mark: Mark\n}\nfunc main() {\n  let p = Pixel(color: .green, mark: .tint(.red))\n  print(p.color == .green)\n  print(.green == p.color)\n  print(p.color != .red)\n  switch p.mark {\n  case .tint(let c): print(c == .red)\n  case .none: print(false)\n  }\n  var q = p\n  q.color = .red\n  print(q.color == Color.red)\n}";
    assert_eq!(run(source), "true\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn an_implicit_case_sees_through_an_optional() {
    let source = "enum Color { case red, green }\nfunc show(_ c: Color?) {\n  if let v = c { print(v == .red) } else { print(0) }\n}\nfunc main() {\n  let a: Color? = .red\n  let b: Color? = .none\n  let n: Int? = .some(4)\n  let m: Int? = .none\n  show(a)\n  show(b)\n  show(.green)\n  if let v = n { print(v) }\n  if m == nil { print(9) }\n}";
    assert_eq!(run(source), "true\n0\nfalse\n4\n9\n");
}

#[test]
fn an_implicit_case_of_a_generic_enum_takes_its_arguments_from_the_context() {
    let source = "enum Maybe<T> {\n  case some(T)\n  case nothing\n}\nfunc unwrap(_ m: Maybe<Int>, or d: Int) -> Int {\n  switch m {\n  case .some(let v): return v\n  case .nothing: return d\n  }\n}\nfunc main() {\n  let a: Maybe<Int> = .some(7)\n  let b: Maybe<Int> = .nothing\n  print(unwrap(a, or: 0))\n  print(unwrap(b, or: 5))\n  print(unwrap(.some(1), or: 0))\n}";
    assert_eq!(run(source), "7\n5\n1\n");
}

#[test]
fn an_implicit_case_is_checked_against_the_enum_of_the_context() {
    let source = format!(
        "{SHAPES_ENUM}func main() {{\n  let s: Shape = .tinted(.green)\n  print(area(s))\n  print(area(.tinted(.red)))\n}}"
    );
    assert_eq!(run(&source), "2\n1\n");
}

#[test]
fn an_implicit_case_without_a_context_is_rejected() {
    assert!(build_error("enum E { case a }\nfunc main() {\n  let x = .a\n}").contains("context"));
    assert!(build_error("enum E { case a }\nfunc main() {\n  print(.a)\n}").contains("context"));
}

#[test]
fn an_implicit_case_needs_an_enum_context_that_has_the_case() {
    let not_enum = "func main() {\n  let n: Int = .a\n}";
    assert!(build_error(not_enum).contains("enum"));
    let unknown = "enum E { case a }\nfunc main() {\n  let e: E = .b\n}";
    assert!(build_error(unknown).contains("'b'"));
    let optional_of_int = "func main() {\n  let n: Int? = .a\n}";
    assert!(!build_error(optional_of_int).is_empty());
}

#[test]
fn an_implicit_case_with_values_follows_the_rules_of_a_qualified_one() {
    let missing = "enum E { case a(Int) }\nfunc main() {\n  let e: E = .a\n}";
    assert!(build_error(missing).contains("carries values"));
    let count = "enum E { case a(Int) }\nfunc main() {\n  let e: E = .a(1, 2)\n}";
    assert!(build_error(count).contains("takes 1 value"));
    let wrong = "enum E { case a(Int) }\nfunc main() {\n  let e: E = .a(true)\n}";
    assert!(build_error(wrong).contains("expected Int"));
    let extra = "enum E { case a }\nfunc main() {\n  let e: E = .a(1)\n}";
    assert!(!build_error(extra).is_empty());
}

#[test]
fn a_local_named_like_the_enum_does_not_hide_an_implicit_case() {
    let source = "enum Color { case red, green }\nfunc main() {\n  let Color = 5\n  let c: Color = .red\n  print(c == .red)\n  print(Color)\n}";
    assert_eq!(run(source), "true\n5\n");
}

const GENERIC_BASE: &str = "class Base<T> {\n  var value: T\n  func get() -> T { return value }\n  func set(_ v: T) { value = v }\n}\n";

#[test]
fn a_generic_class_may_extend_a_generic_class() {
    let source = format!(
        "{GENERIC_BASE}class Derived<T>: Base<T> {{\n  var extra: Int\n  func describe() -> Int {{ return extra * 10 }}\n}}\nfunc main() {{\n  let d = Derived<Int>(value: 3, extra: 4)\n  print(d.get())\n  print(d.value)\n  print(d.describe())\n  d.set(9)\n  print(d.get())\n  let s = Derived<String>(value: \"s\", extra: 1)\n  print(s.get())\n}}"
    );
    assert_eq!(run(&source), "3\n3\n40\n9\ns\n");
}

#[test]
fn a_plain_class_may_extend_an_instance_of_a_generic_class() {
    let source = format!(
        "{GENERIC_BASE}class IntBox: Base<Int> {{\n  func doubled() -> Int {{ return get() * 2 }}\n}}\nfunc main() {{\n  let b = IntBox(value: 21)\n  print(b.doubled())\n  b.set(4)\n  print(b.doubled())\n  print(b.value + 1)\n}}"
    );
    assert_eq!(run(&source), "42\n8\n5\n");
}

#[test]
fn a_subclass_may_add_type_parameters_or_fix_those_of_its_superclass() {
    let source = format!(
        "{GENERIC_BASE}class Pair<A, B>: Base<A> {{\n  var second: B\n  func other() -> B {{ return second }}\n}}\nclass Tagged<T>: Base<Int> {{\n  var tag: T\n}}\nfunc main() {{\n  let p = Pair<Int, String>(value: 1, second: \"two\")\n  print(p.get())\n  print(p.other())\n  let t = Tagged<Bool>(value: 5, tag: true)\n  print(t.get())\n  print(t.tag)\n}}"
    );
    assert_eq!(run(&source), "1\ntwo\n5\ntrue\n");
}

#[test]
fn an_instance_of_a_generic_subclass_is_usable_as_its_superclass() {
    let source = format!(
        "{GENERIC_BASE}class Derived<T>: Base<T> {{\n  var extra: Int\n}}\nfunc first<T>(_ b: Base<T>) -> T {{ return b.get() }}\nfunc read(_ b: Base<Int>) -> Int {{ return b.get() + b.value }}\nfunc main() {{\n  let d = Derived<Int>(value: 3, extra: 0)\n  let up: Base<Int> = d\n  print(up.get())\n  print(read(d))\n  print(first(d))\n  let s = Derived<String>(value: \"x\", extra: 0)\n  print(first(s))\n}}"
    );
    assert_eq!(run(&source), "3\n6\n3\nx\n");
}

#[test]
fn a_generic_subclass_initializer_calls_the_superclass_initializer() {
    let source = "class Base<T> {\n  var value: T\n  init(value: T) { self.value = value }\n  func get() -> T { return value }\n}\nclass Derived<T>: Base<T> {\n  var extra: Int\n  init(v: T, extra: Int) {\n    self.extra = extra\n    super.init(value: v)\n  }\n  func both() -> T { return super.get() }\n}\nclass Fixed: Base<Int> {\n  init() { super.init(value: 7) }\n}\nfunc main() {\n  let d = Derived(v: \"q\", extra: 2)\n  print(d.both())\n  print(d.extra)\n  print(Fixed().get())\n}";
    assert_eq!(run(source), "q\n2\n7\n");
}

#[test]
fn members_of_a_generic_hierarchy_are_counted_and_released() {
    let source = format!(
        "{TRACKED}class Base<T> {{\n  var item: T\n}}\nclass Holder<T>: Base<T> {{\n  var extra: Tracked\n}}\nfunc main() {{\n  let h = Holder<Tracked>(item: Tracked(id: 1), extra: Tracked(id: 2))\n  print(h.item.id + h.extra.id)\n}}"
    );
    assert_eq!(run(&source), "3\n-1\n-2\n");
}

#[test]
fn a_generic_class_cannot_inherit_from_a_struct_from_itself_or_with_the_wrong_arguments() {
    let from_struct = "struct S<T> { var x: T }\nclass C<T>: S<T> { var y: T }\nfunc main() { }";
    assert!(build_error(from_struct).contains("not a class"));
    let itself = "class A<T>: A<T> { var x: T }\nfunc main() { }";
    assert!(build_error(itself).contains("itself"));
    let arity = "class A<T> { var x: T }\nclass B: A<Int, Bool> { var y: Int }\nfunc main() { }";
    assert!(build_error(arity).contains("expects 1 type argument"));
    let bare = "class A<T> { var x: T }\nclass B: A { var y: Int }\nfunc main() { }";
    assert!(!build_error(bare).is_empty());
}

const GENERIC_CHAIN: &str = "class Base<T> {\n  var value: T\n  func describe() -> Int { return 1 }\n  func get() -> T { return value }\n  func twice() -> Int { return describe() * 2 }\n}\n";

#[test]
fn a_plain_subclass_may_override_a_method_of_a_generic_superclass() {
    let source = format!(
        "{GENERIC_CHAIN}class IntBox: Base<Int> {{\n  override func describe() -> Int {{ return 2 }}\n}}\nfunc show(_ b: Base<Int>) {{\n  print(b.describe())\n  print(b.twice())\n}}\nfunc main() {{\n  show(Base<Int>(value: 0))\n  show(IntBox(value: 0))\n}}"
    );
    assert_eq!(run(&source), "1\n2\n2\n4\n");
}

#[test]
fn a_generic_subclass_may_override_a_method_and_the_override_sees_its_type_parameter() {
    let source = format!(
        "{GENERIC_CHAIN}class Derived<T>: Base<T> {{\n  var fallback: T\n  override func get() -> T {{ return fallback }}\n  override func describe() -> Int {{ return super.describe() + 10 }}\n}}\nfunc read<T>(_ b: Base<T>) -> T {{ return b.get() }}\nfunc main() {{\n  let a: Base<String> = Derived<String>(value: \"v\", fallback: \"f\")\n  let b: Base<Int> = Derived<Int>(value: 1, fallback: 2)\n  let c: Base<Int> = Base<Int>(value: 3)\n  print(read(a))\n  print(read(b))\n  print(read(c))\n  print(a.describe())\n  print(b.twice())\n}}"
    );
    assert_eq!(run(&source), "f\n2\n3\n11\n22\n");
}

#[test]
fn dispatch_through_a_generic_class_uses_the_class_of_the_object_whoever_created_it() {
    let source = format!(
        "{GENERIC_CHAIN}class A: Base<Int> {{\n  override func describe() -> Int {{ return 7 }}\n}}\nclass B<T>: Base<T> {{\n  override func describe() -> Int {{ return 8 }}\n}}\nfunc makeA() -> Base<Int> {{ return A(value: 0) }}\nfunc makeB() -> Base<Int> {{ return B<Int>(value: 0) }}\nfunc makeBase() -> Base<Int> {{ return Base<Int>(value: 0) }}\nfunc use(_ b: Base<Int>) -> Int {{ return b.describe() }}\nfunc main() {{\n  print(use(makeA()))\n  print(use(makeB()))\n  print(use(makeBase()))\n}}"
    );
    assert_eq!(run(&source), "7\n8\n1\n");
}

#[test]
fn a_generic_hierarchy_of_several_levels_keeps_its_methods_in_one_table() {
    let source = "class A<T> {\n  var v: T\n  func f() -> Int { return 1 }\n  func g() -> Int { return 10 }\n}\nclass B<T>: A<T> {\n  func h() -> Int { return 100 }\n  override func g() -> Int { return 20 }\n}\nclass C<T>: B<T> {\n  override func f() -> Int { return 2 }\n  override func h() -> Int { return 200 }\n  func k() -> Int { return 1000 }\n}\nfunc total(_ a: A<Int>) -> Int { return a.f() * 1 + a.g() }\nfunc more(_ b: B<Int>) -> Int { return b.h() + b.f() }\nfunc main() {\n  let c = C<Int>(v: 0)\n  let b = B<Int>(v: 0)\n  print(total(c))\n  print(total(b))\n  print(more(c))\n  print(more(b))\n  print(c.k())\n}";
    assert_eq!(run(source), "22\n21\n202\n101\n1000\n");
}

#[test]
fn a_protocol_requirement_is_dispatched_dynamically_on_a_generic_class_hierarchy() {
    let source = format!(
        "protocol Named {{\n  func describe() -> Int\n}}\n{GENERIC_CHAIN}class IntBox: Base<Int>, Named {{\n  override func describe() -> Int {{ return 5 }}\n}}\nfunc show<N: Named>(_ n: N) -> Int {{ return n.describe() }}\nfunc main() {{\n  print(show(IntBox(value: 0)))\n}}"
    );
    assert_eq!(run(&source), "5\n");
}

#[test]
fn casts_work_on_a_hierarchy_that_includes_generic_classes() {
    let source = format!(
        "{GENERIC_CHAIN}class IntBox: Base<Int> {{ }}\nclass Wide<T>: Base<T> {{\n  var more: Int\n}}\nfunc main() {{\n  let a: Base<Int> = IntBox(value: 1)\n  let b: Base<Int> = Wide<Int>(value: 2, more: 9)\n  let c: Base<Int> = Base<Int>(value: 3)\n  print(a is IntBox)\n  print(b is IntBox)\n  print(b is Wide<Int>)\n  print(c is Wide<Int>)\n  print(a is Base<Int>)\n  if let w = b as? Wide<Int> {{ print(w.more) }}\n  if let w = a as? Wide<Int> {{ print(w.more) }} else {{ print(0) }}\n  let forced = b as! Wide<Int>\n  print(forced.more + 1)\n}}"
    );
    assert_eq!(run(&source), "true\nfalse\ntrue\nfalse\ntrue\n9\n0\n10\n");
}

#[test]
fn an_instance_of_a_subclass_that_overrides_is_released_through_its_own_class() {
    let source = format!(
        "{TRACKED}class Base<T> {{\n  var item: T\n  func tag() -> Int {{ return 1 }}\n}}\nclass Sub: Base<Tracked> {{\n  var own: Tracked\n  override func tag() -> Int {{ return 2 }}\n}}\nfunc main() {{\n  let b: Base<Tracked> = Sub(item: Tracked(id: 1), own: Tracked(id: 2))\n  print(b.tag())\n}}"
    );
    assert_eq!(run(&source), "2\n-1\n-2\n");
}

#[test]
fn an_override_must_keep_the_signature_with_the_type_arguments_of_the_superclass() {
    let source = format!(
        "{GENERIC_CHAIN}class IntBox: Base<Int> {{\n  override func get() -> Bool {{ return true }}\n}}\nfunc main() {{ }}"
    );
    assert!(build_error(&source).contains("signature"));
}

#[test]
fn a_method_with_type_parameters_of_its_own_cannot_be_overridden() {
    let source = "class Base {\n  func pick<U>(_ x: U) -> U { return x }\n}\nclass Sub: Base {\n  override func pick<U>(_ x: U) -> U { return x }\n}\nfunc main() { }";
    assert!(build_error(source).contains("type parameters of its own"));
}

const LABELED: &str = "enum Shape {\n  case circle(radius: Int)\n  case rect(width: Int, height: Int)\n  case tag(Int, name: String)\n  case empty\n}\n";

#[test]
fn a_payload_value_may_carry_a_label_in_the_declaration_and_the_construction() {
    let source = format!(
        "{LABELED}func area(_ s: Shape) -> Int {{\n  switch s {{\n  case .circle(let r): return 3 * r * r\n  case .rect(let w, let h): return w * h\n  case .tag(let n, let name): return n + name.count\n  case .empty: return 0\n  }}\n}}\nfunc main() {{\n  print(area(Shape.circle(radius: 2)))\n  print(area(.rect(width: 3, height: 4)))\n  print(area(Shape.tag(1, name: \"abc\")))\n  let s: Shape = .rect(width: 5, height: 5)\n  print(area(s))\n}}"
    );
    assert_eq!(run(&source), "12\n12\n4\n25\n");
}

#[test]
fn a_pattern_may_repeat_the_labels_or_leave_them_out() {
    let source = format!(
        "{LABELED}func main() {{\n  let s: Shape = .rect(width: 2, height: 7)\n  switch s {{\n  case .rect(width: let w, height: let h): print(w * 10 + h)\n  default: print(0)\n  }}\n  switch s {{\n  case .rect(width: _, height: let h): print(h)\n  default: print(0)\n  }}\n  switch s {{\n  case .rect(let w, let h): print(w + h)\n  default: print(0)\n  }}\n  let t: Shape = .tag(4, name: \"x\")\n  switch t {{\n  case .tag(let n, name: let name): print(n + name.count)\n  default: print(0)\n  }}\n}}"
    );
    assert_eq!(run(&source), "27\n7\n9\n5\n");
}

#[test]
fn the_labels_of_a_generic_payload_are_kept_when_the_arguments_are_inferred() {
    let source = "enum Pair<T> {\n  case both(first: T, second: T)\n  case none\n}\nfunc sum(_ p: Pair<Int>) -> Int {\n  switch p {\n  case .both(first: let a, second: let b): return a + b\n  case .none: return 0\n  }\n}\nfunc main() {\n  let p = Pair.both(first: 3, second: 4)\n  print(sum(p))\n  print(sum(.both(first: 1, second: 1)))\n  print(sum(.none))\n}";
    assert_eq!(run(source), "7\n2\n0\n");
}

#[test]
fn a_payload_label_must_be_written_where_the_case_declares_it() {
    let missing = format!("{LABELED}func main() {{\n  let s = Shape.circle(2)\n}}");
    assert!(build_error(&missing).contains("radius"));
    let wrong = format!("{LABELED}func main() {{\n  let s = Shape.circle(size: 2)\n}}");
    assert!(build_error(&wrong).contains("radius"));
    let swapped = format!("{LABELED}func main() {{\n  let s = Shape.rect(height: 1, width: 2)\n}}");
    assert!(build_error(&swapped).contains("width"));
    let implicit = format!("{LABELED}func main() {{\n  let s: Shape = .circle(2)\n}}");
    assert!(build_error(&implicit).contains("radius"));
}

#[test]
fn a_payload_without_a_label_cannot_be_given_one() {
    let source = "enum E { case a(Int) }\nfunc main() {\n  let e = E.a(value: 1)\n}";
    assert!(build_error(source).contains("label"));
    let mixed = format!("{LABELED}func main() {{\n  let s = Shape.tag(num: 1, name: \"x\")\n}}");
    assert!(build_error(&mixed).contains("label"));
}

#[test]
fn a_pattern_cannot_name_a_label_that_the_case_does_not_have() {
    let wrong = format!(
        "{LABELED}func main() {{\n  let s: Shape = .empty\n  switch s {{\n  case .circle(size: let r): print(r)\n  default: print(0)\n  }}\n}}"
    );
    assert!(build_error(&wrong).contains("radius"));
    let unlabeled = "enum E { case a(Int) }\nfunc main() {\n  let e = E.a(1)\n  switch e {\n  case .a(value: let v): print(v)\n  }\n}";
    assert!(build_error(unlabeled).contains("label"));
}

const MULTI: &str = "enum Shape {\n  case circle(Int)\n  case square(Int)\n  case rect(Int, Int)\n  case pair(Int, Int)\n  case label(String)\n  case empty\n}\n";

#[test]
fn one_case_may_list_several_patterns_that_bind_the_same_name() {
    let source = format!(
        "{MULTI}func size(_ s: Shape) -> Int {{\n  switch s {{\n  case .circle(let r), .square(let r): return r * 10\n  case .rect(let w, _), .pair(_, let w): return w\n  case .label(let t): return t.count\n  case .empty: return 0\n  }}\n}}\nfunc main() {{\n  print(size(.circle(2)))\n  print(size(.square(3)))\n  print(size(.rect(4, 9)))\n  print(size(.pair(9, 5)))\n  print(size(.label(\"abc\")))\n  print(size(.empty))\n}}"
    );
    assert_eq!(run(&source), "20\n30\n4\n5\n3\n0\n");
}

#[test]
fn a_shared_body_may_declare_locals_return_and_use_the_bound_name_many_times() {
    let source = format!(
        "{MULTI}func describe(_ s: Shape) -> Int {{\n  switch s {{\n  case .circle(let n), .square(let n), .rect(_, let n):\n    var total = 0\n    var i = 0\n    while i < n {{\n      total += i\n      i += 1\n    }}\n    if total > 5 {{ return total }}\n    return 0 - total\n  default:\n    return 99\n  }}\n}}\nfunc main() {{\n  print(describe(.circle(4)))\n  print(describe(.square(2)))\n  print(describe(.rect(1, 5)))\n  print(describe(.empty))\n}}"
    );
    assert_eq!(run(&source), "6\n-1\n10\n99\n");
}

#[test]
fn a_switch_over_several_patterns_with_bindings_is_exhaustive_without_a_default() {
    let source = "enum E {\n  case a(Int)\n  case b(Int)\n  case c(Int)\n}\nfunc f(_ e: E) -> Int {\n  switch e {\n  case .a(let x), .b(let x): return x\n  case .c(let y): return y + 100\n  }\n}\nfunc main() {\n  print(f(.a(1)))\n  print(f(.b(2)))\n  print(f(.c(3)))\n}";
    assert_eq!(run(source), "1\n2\n103\n");
}

#[test]
fn patterns_of_one_case_may_repeat_labels_and_mix_with_patterns_without_bindings() {
    let source = "enum E {\n  case a(x: Int)\n  case b(y: Int, z: Int)\n  case c\n  case d\n}\nfunc f(_ e: E) -> Int {\n  switch e {\n  case .a(x: let v), .b(y: _, z: let v): return v\n  case .c, .d: return 0\n  }\n}\nfunc main() {\n  print(f(.a(x: 5)))\n  print(f(.b(y: 1, z: 7)))\n  print(f(.c))\n}";
    assert_eq!(run(source), "5\n7\n0\n");
}

#[test]
fn a_class_bound_by_several_patterns_is_released_once() {
    let source = format!(
        "{TRACKED}enum Holder {{\n  case first(Tracked)\n  case second(Tracked, Int)\n}}\nfunc main() {{\n  let h = Holder.second(Tracked(id: 1), 0)\n  switch h {{\n  case .first(let t), .second(let t, _):\n    print(t.id)\n  }}\n  print(9)\n}}"
    );
    assert_eq!(run(&source), "1\n9\n-1\n");
}

#[test]
fn a_generic_enum_may_bind_one_name_in_several_patterns() {
    let source = "enum Either<T> {\n  case left(T)\n  case right(T)\n  case both(T, T)\n}\nfunc first<T>(_ e: Either<T>) -> T {\n  switch e {\n  case .left(let v), .right(let v), .both(let v, _): return v\n  }\n}\nfunc main() {\n  print(first(Either.left(1)))\n  print(first(Either.right(2)))\n  print(first(Either.both(3, 4)))\n  print(first(Either.right(\"s\")))\n}";
    assert_eq!(run(source), "1\n2\n3\ns\n");
}

#[test]
fn the_patterns_of_a_case_must_bind_the_same_names_with_the_same_types() {
    let names = format!(
        "{MULTI}func f(_ s: Shape) {{\n  switch s {{\n  case .circle(let a), .square(let b): print(a)\n  default: print(0)\n  }}\n}}\nfunc main() {{ }}"
    );
    assert!(build_error(&names).contains("same names"));
    let types = format!(
        "{MULTI}func f(_ s: Shape) {{\n  switch s {{\n  case .circle(let a), .label(let a): print(1)\n  default: print(0)\n  }}\n}}\nfunc main() {{ }}"
    );
    assert!(build_error(&types).contains("type"));
    let mixed = format!(
        "{MULTI}func f(_ s: Shape) {{\n  switch s {{\n  case .circle(let a), .empty: print(a)\n  default: print(0)\n  }}\n}}\nfunc main() {{ }}"
    );
    assert!(build_error(&mixed).contains("same names"));
}

#[test]
fn a_name_cannot_be_bound_twice_by_one_pattern() {
    let source = format!(
        "{MULTI}func f(_ s: Shape) {{\n  switch s {{\n  case .rect(let a, let a): print(a)\n  default: print(0)\n  }}\n}}\nfunc main() {{ }}"
    );
    assert!(build_error(&source).contains("already declared"));
}

#[test]
fn bound_names_are_not_visible_outside_their_case() {
    let source = format!(
        "{MULTI}func f(_ s: Shape) {{\n  switch s {{\n  case .circle(let a), .square(let a): print(a)\n  default: print(a)\n  }}\n}}\nfunc main() {{ }}"
    );
    assert!(build_error(&source).contains("cannot find 'a'"));
}

#[test]
fn a_generic_class_may_declare_a_deinit() {
    let source = "class Counted<T> {\n  var item: T\n  var id: Int\n  deinit { print(0 - id) }\n}\nfunc main() {\n  let a = Counted<Int>(item: 5, id: 1)\n  let b = Counted<String>(item: \"s\", id: 2)\n  print(a.item)\n  print(b.item)\n}";
    assert_eq!(run(source), "5\ns\n-2\n-1\n");
}

#[test]
fn each_instance_of_a_generic_class_runs_its_deinit_when_the_last_reference_goes() {
    let source = "class Counted<T> {\n  var item: T\n  var id: Int\n  deinit { print(0 - id) }\n}\nfunc main() {\n  var c = Counted<Int>(item: 1, id: 1)\n  print(10)\n  c = Counted<Int>(item: 2, id: 2)\n  print(20)\n}";
    assert_eq!(run(source), "10\n-1\n20\n-2\n");
}

#[test]
fn a_deinit_of_a_generic_class_runs_before_that_of_its_superclass_and_before_its_members_go() {
    let source = format!(
        "{TRACKED}class Base<T> {{\n  var item: T\n  deinit {{ print(1) }}\n}}\nclass Derived<T>: Base<T> {{\n  var own: Tracked\n  deinit {{ print(2) }}\n}}\nfunc main() {{\n  let d = Derived<Int>(item: 0, own: Tracked(id: 7))\n  print(d.item)\n}}"
    );
    assert_eq!(run(&source), "0\n2\n1\n-7\n");
}

#[test]
fn a_plain_subclass_of_a_generic_class_runs_both_deinits_and_one_that_adds_none_inherits_the_base_one()
 {
    let source = "class Base<T> {\n  var item: T\n  deinit { print(1) }\n}\nclass Sub: Base<Int> {\n  deinit { print(2) }\n}\nclass Quiet: Base<Bool> { }\nfunc main() {\n  let s = Sub(item: 0)\n  let q = Quiet(item: true)\n  print(s.item)\n}";
    assert_eq!(run(source), "0\n1\n2\n1\n");
}

#[test]
fn a_deinit_of_a_generic_class_may_use_the_constraint_of_its_parameter() {
    let source = "protocol Shape {\n  func area() -> Int\n}\nstruct Square: Shape {\n  var side: Int\n  func area() -> Int { return side * side }\n}\nclass Holder<T: Shape> {\n  var item: T\n  deinit { print(item.area()) }\n}\nfunc main() {\n  let h = Holder(item: Square(side: 3))\n  print(0)\n}";
    assert_eq!(run(source), "0\n9\n");
}

#[test]
fn a_generic_class_created_inside_a_generic_function_runs_its_deinit() {
    let source = "class Counted<T> {\n  var item: T\n  deinit { print(7) }\n}\nfunc make<U>(_ x: U) -> Counted<U> { return Counted<U>(item: x) }\nfunc use<U>(_ x: U) {\n  let c = make(x)\n  print(1)\n}\nfunc main() {\n  use(5)\n  use(true)\n}";
    assert_eq!(run(source), "1\n7\n1\n7\n");
}

#[test]
fn a_deinit_of_a_generic_class_may_not_return_a_value_or_take_parameters() {
    let source = "class C<T> {\n  var item: T\n  deinit() { }\n}\nfunc main() { }";
    assert!(!build_error(source).is_empty());
}

#[test]
fn two_optionals_are_equal_when_both_are_nil_or_both_hold_equal_values() {
    let source = "func main() {\n  let a: Int? = 5\n  let b: Int? = 5\n  let c: Int? = 6\n  let n: Int? = nil\n  let m: Int? = nil\n  print(a == b)\n  print(a == c)\n  print(a == n)\n  print(n == a)\n  print(n == m)\n  print(a != b)\n  print(a != c)\n  print(a != n)\n  print(n != m)\n}";
    assert_eq!(
        run(source),
        "true\nfalse\nfalse\nfalse\ntrue\nfalse\ntrue\ntrue\nfalse\n"
    );
}

#[test]
fn an_optional_may_be_compared_with_a_plain_value_on_either_side() {
    let source = "func main() {\n  let a: Int? = 5\n  let n: Int? = nil\n  print(a == 5)\n  print(5 == a)\n  print(a != 5)\n  print(a == 6)\n  print(n == 5)\n  print(5 != n)\n  let s: String? = \"x\"\n  print(s == \"x\")\n  print(s == \"y\")\n}";
    assert_eq!(
        run(source),
        "true\ntrue\nfalse\nfalse\nfalse\ntrue\ntrue\nfalse\n"
    );
}

#[test]
fn optionals_of_strings_booleans_enums_and_structs_can_be_compared() {
    let source = "enum Color { case red, green }\nstruct P {\n  var x: Int\n  var tag: String\n}\nfunc main() {\n  let s1: String? = \"a\" + \"b\"\n  let s2: String? = \"ab\"\n  let b1: Bool? = true\n  let c1: Color? = .red\n  let c2: Color? = .green\n  let p1: P? = P(x: 1, tag: \"t\")\n  let p2: P? = P(x: 1, tag: \"t\")\n  let p3: P? = P(x: 2, tag: \"t\")\n  print(s1 == s2)\n  print(b1 == true)\n  print(c1 == c2)\n  print(c1 == .red)\n  print(p1 == p2)\n  print(p1 == p3)\n  print(p1 == nil)\n}";
    assert_eq!(run(source), "true\ntrue\nfalse\ntrue\ntrue\nfalse\nfalse\n");
}

#[test]
fn a_nested_optional_is_compared_level_by_level() {
    let source = "func main() {\n  let a: Int?? = 5\n  let b: Int?? = 5\n  let c: Int?? = .some(nil)\n  let d: Int?? = nil\n  print(a == b)\n  print(a == c)\n  print(c == d)\n  print(c == c)\n}";
    assert_eq!(run(source), "true\nfalse\nfalse\ntrue\n");
}

#[test]
fn each_operand_of_an_optional_comparison_is_evaluated_once_from_left_to_right() {
    let source = "func make(_ n: Int, _ v: Int?) -> Int? {\n  print(n)\n  return v\n}\nfunc main() {\n  print(make(1, 3) == make(2, 3))\n  print(make(3, nil) != make(4, 4))\n}";
    assert_eq!(run(source), "1\n2\ntrue\n3\n4\ntrue\n");
}

#[test]
fn comparing_optional_strings_in_a_loop_does_not_leak() {
    let source = "func main() {\n  var hits = 0\n  var i = 0\n  while i < 20000 {\n    let a: String? = \"k\" + String(i)\n    let b: String? = \"k\" + String(i)\n    if a == b { hits += 1 }\n    i += 1\n  }\n  print(hits)\n}";
    assert_eq!(run(source), "20000\n");
}

#[test]
fn an_optional_satisfies_equatable_when_its_wrapped_type_does() {
    let source = "func same<T: Equatable>(_ a: T, _ b: T) -> Bool { return a == b }\nfunc maybe<T: Equatable>(_ a: T?, _ b: T?) -> Bool { return a == b }\nfunc main() {\n  let a: Int? = 1\n  let n: Int? = nil\n  print(same(a, a))\n  print(same(a, n))\n  print(same(n, n))\n  print(maybe(a, 1))\n  print(maybe(\"s\", \"s\"))\n  print(maybe(n, n))\n  print(maybe(1, 2))\n}";
    assert_eq!(run(source), "true\nfalse\ntrue\ntrue\ntrue\ntrue\nfalse\n");
}

#[test]
fn a_struct_with_an_optional_member_can_be_compared() {
    let source = "struct P: Equatable {\n  var x: Int?\n  var s: String?\n}\nfunc main() {\n  let a = P(x: 1, s: nil)\n  let b = P(x: 1, s: nil)\n  let c = P(x: nil, s: \"s\")\n  let d = P(x: nil, s: \"s\")\n  print(a == b)\n  print(a == c)\n  print(c == d)\n  print(c != d)\n}";
    assert_eq!(run(source), "true\nfalse\ntrue\nfalse\n");
}

#[test]
fn optionals_of_unequatable_types_cannot_be_compared() {
    let class = "class C { var x: Int }\nfunc main() {\n  let a: C? = C(x: 1)\n  print(a == a)\n}";
    assert!(build_error(class).contains("equality is not supported"));
    let mismatch = "func main() {\n  let a: Int? = 1\n  let s: String? = \"s\"\n  print(a == s)\n}";
    assert!(build_error(mismatch).contains("expected"));
    let unconstrained = "func f<T>(_ a: T?, _ b: T?) -> Bool { return a == b }\nfunc main() { }";
    assert!(!build_error(unconstrained).is_empty());
}

#[test]
fn a_string_literal_in_a_character_context_is_a_character() {
    let source = "func show(_ c: Character) { print(c) }\nfunc pick() -> Character { return \"z\" }\nfunc main() {\n  let a: Character = \"a\"\n  var b: Character = \"b\"\n  b = \"c\"\n  print(a)\n  print(b)\n  show(\"d\")\n  print(pick())\n  print(\"é\")\n}";
    assert_eq!(run(source), "a\nc\nd\nz\né\n");
}

#[test]
fn characters_are_compared_for_equality_and_order() {
    let source = "func main() {\n  let a: Character = \"a\"\n  let b: Character = \"b\"\n  print(a == a)\n  print(a == b)\n  print(a != b)\n  print(a < b)\n  print(b <= a)\n  print(a == \"a\")\n  print(\"b\" > a)\n  let e: Character = \"é\"\n  print(e > \"z\")\n}";
    assert_eq!(
        run(source),
        "true\nfalse\ntrue\ntrue\nfalse\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn a_character_becomes_a_string_with_its_utf8_bytes() {
    let source = "func main() {\n  let a: Character = \"a\"\n  let e: Character = \"é\"\n  let j: Character = \"あ\"\n  let s: Character = \"😀\"\n  print(String(a).count)\n  print(String(e).count)\n  print(String(j).count)\n  print(String(s).count)\n  print(String(e) + String(j) + String(s))\n  print(\"[\\(a)\\(e)]\")\n  print(String(a) == \"a\")\n}";
    assert_eq!(run(source), "1\n2\n3\n4\néあ😀\n[aé]\ntrue\n");
}

#[test]
fn a_for_loop_visits_the_characters_of_a_string() {
    let source = "func main() {\n  for c in \"héあ😀!\" {\n    print(c)\n  }\n  var count = 0\n  for c in \"abc\" + \"def\" {\n    count += 1\n    let ignored = c\n  }\n  print(count)\n  for c in \"\" { print(\"never\") }\n  var out = \"\"\n  for c in \"xyz\" { out += String(c) + \"-\" }\n  print(out)\n}";
    assert_eq!(run(source), "h\né\nあ\n😀\n!\n6\nx-y-z-\n");
}

#[test]
fn break_and_continue_work_in_a_loop_over_a_string() {
    let source = "func main() {\n  for c in \"abcde\" {\n    if c == \"b\" { continue }\n    if c == \"d\" { break }\n    print(c)\n  }\n  outer: for a in \"xy\" {\n    for b in \"12\" {\n      if b == \"2\" { continue outer }\n      print(String(a) + String(b))\n    }\n  }\n}";
    assert_eq!(run(source), "a\nc\nx1\ny1\n");
}

#[test]
fn a_switch_on_a_character_takes_literals_and_ranges() {
    let source = "func kind(_ c: Character) -> Int {\n  switch c {\n  case \"a\", \"e\", \"i\": return 1\n  case \"a\"...\"z\": return 2\n  case \"0\"...\"9\": return 3\n  default: return 0\n  }\n}\nfunc main() {\n  for c in \"ab5-e\" { print(kind(c)) }\n}";
    assert_eq!(run(source), "1\n2\n3\n0\n1\n");
}

#[test]
fn a_character_may_be_stored_optional_generic_and_compared_through_comparable() {
    let source = "struct Pair: Equatable {\n  var left: Character\n  var right: Character?\n}\nfunc largest<T: Comparable>(_ a: T, _ b: T) -> T { if a > b { return a }; return b }\nfunc main() {\n  let a: Character = \"a\"\n  let z: Character = \"z\"\n  print(largest(a, z))\n  let o: Character? = a\n  print(o == a)\n  print(o == nil)\n  let p = Pair(left: a, right: nil)\n  let q = Pair(left: a, right: nil)\n  print(p == q)\n}";
    assert_eq!(run(source), "z\ntrue\nfalse\ntrue\n");
}

#[test]
fn a_character_knows_its_ascii_class_and_its_value() {
    let source = "func main() {\n  for c in \"aZ5 !é\" {\n    print(c.isLetter)\n    print(c.isNumber)\n    print(c.isWhitespace)\n    print(c.isUppercase)\n    print(c.isLowercase)\n    print(c.unicodeValue)\n  }\n}";
    let expected = [
        ("a", "true", "false", "false", "false", "true", "97"),
        ("Z", "true", "false", "false", "true", "false", "90"),
        ("5", "false", "true", "false", "false", "false", "53"),
        (" ", "false", "false", "true", "false", "false", "32"),
        ("!", "false", "false", "false", "false", "false", "33"),
        ("é", "false", "false", "false", "false", "false", "233"),
    ];
    let text: String = expected
        .iter()
        .map(|(_, l, n, w, u, lo, v)| format!("{l}\n{n}\n{w}\n{u}\n{lo}\n{v}\n"))
        .collect();
    assert_eq!(run(source), text);
}

#[test]
fn a_character_can_be_made_from_a_value_or_a_one_character_string() {
    let source = "func main() {\n  print(Character(65))\n  print(Character(233))\n  print(Character(128512))\n  let s = \"q\" + \"\"\n  print(Character(s))\n  print(Character(\"w\"))\n}";
    assert_eq!(run(source), "A\né\n😀\nq\nw\n");
}

#[test]
fn an_invalid_character_value_or_string_traps() {
    run_expecting_a_crash("func main() {\n  let n = 55296\n  print(Character(n))\n}");
    run_expecting_a_crash("func main() {\n  let n = -1\n  print(Character(n))\n}");
    run_expecting_a_crash("func main() {\n  let s = \"ab\" + \"\"\n  print(Character(s))\n}");
    run_expecting_a_crash("func main() {\n  let s = \"\" + \"\"\n  print(Character(s))\n}");
}

#[test]
fn a_character_literal_must_hold_exactly_one_character() {
    assert!(build_error("func main() {\n  let c: Character = \"ab\"\n}").contains("exactly one"));
    assert!(build_error("func main() {\n  let c: Character = \"\"\n}").contains("exactly one"));
    assert!(build_error("func main() {\n  print(Character(\"ab\"))\n}").contains("exactly one"));
}

#[test]
fn a_character_does_not_mix_with_strings_or_numbers() {
    let a = "func main() {\n  let a: Character = \"a\"\n  let s = \"s\"\n  print(a == s)\n}";
    assert!(build_error(a).contains("expected"));
    assert!(
        build_error("func main() {\n  let a: Character = \"a\"\n  print(a + a)\n}")
            .contains("expected")
    );
    assert!(
        build_error("func main() {\n  let a: Character = \"a\"\n  print(a == 97)\n}")
            .contains("expected")
    );
    assert!(build_error("func main() {\n  let n: Int = \"a\"\n}").contains("expected"));
    assert!(
        build_error("func main() {\n  let a: Character = \"a\"\n  print(a.shout)\n}")
            .contains("shout")
    );
}

#[test]
fn a_string_has_a_first_and_a_last_character_or_none() {
    let source = "func main() {\n  let s = \"héllo😀\"\n  if let f = s.first { print(f) }\n  if let l = s.last { print(l) }\n  let e = \"\" + \"\"\n  print(e.first == nil)\n  print(e.last == nil)\n  print(s.first == \"h\")\n  print(\"é\".last == \"é\")\n  print(\"x\".first == \"x\")\n  print((\"a\" + \"bc\").last == \"c\")\n}";
    assert_eq!(run(source), "h\n😀\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn a_string_can_be_reversed_by_characters() {
    let source = "func main() {\n  print(\"abc\".reversed())\n  print(\"héあ😀\".reversed())\n  print(\"\".reversed().isEmpty)\n  print(\"x\".reversed())\n  let s = \"ab\" + \"cd\"\n  print(s.reversed().reversed() == s)\n  print(\"héあ\".reversed().count)\n}";
    assert_eq!(run(source), "cba\n😀あéh\ntrue\nx\ntrue\n6\n");
}

#[test]
fn a_string_changes_the_case_of_its_ascii_letters_only() {
    let source = "func main() {\n  let s = \"Hello, World 123 é\"\n  print(s.uppercased())\n  print(s.lowercased())\n  print(\"\".uppercased().isEmpty)\n  print((\"ab\" + \"Cd\").uppercased())\n  let c: Character = \"q\"\n  print(c.uppercased())\n  print(\"Z\".lowercased())\n}";
    assert_eq!(
        run(source),
        "HELLO, WORLD 123 é\nhello, world 123 é\ntrue\nABCD\nQ\nz\n"
    );
}

#[test]
fn reversing_and_changing_case_in_a_loop_does_not_leak() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    let s = \"item\" + String(i)\n    total += s.reversed().uppercased().count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "168890\n");
}

#[test]
fn the_new_string_members_reject_misuse() {
    assert!(build_error("func main() { print(\"a\".reversed(1)) }").contains("reversed"));
    assert!(build_error("func main() { print(\"a\".uppercased(1)) }").contains("uppercased"));
    assert!(!build_error("func main() {\n  let s = \"a\"\n  s.first = \"b\"\n}").is_empty());
    assert!(build_error("func main() { print(\"a\".middle) }").contains("middle"));
}

#[test]
fn prefix_and_suffix_take_a_number_of_characters_from_either_end() {
    let source = "func main() {\n  let s = \"héあ😀z\"\n  print(s.prefix(0).isEmpty)\n  print(s.prefix(1))\n  print(s.prefix(3))\n  print(s.prefix(4))\n  print(s.prefix(5) == s)\n  print(s.prefix(99) == s)\n  print(s.suffix(1))\n  print(s.suffix(2))\n  print(s.suffix(4))\n  print(s.suffix(99) == s)\n  print(s.suffix(0).isEmpty)\n  print(\"\".prefix(3).isEmpty)\n}";
    assert_eq!(
        run(source),
        "true\nh\nhéあ\nhéあ😀\ntrue\ntrue\nz\n😀z\néあ😀z\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn drop_first_and_drop_last_remove_a_number_of_characters() {
    let source = "func main() {\n  let s = \"héあ😀z\"\n  print(s.dropFirst(0) == s)\n  print(s.dropFirst(1))\n  print(s.dropFirst(3))\n  print(s.dropFirst(5).isEmpty)\n  print(s.dropFirst(99).isEmpty)\n  print(s.dropLast(0) == s)\n  print(s.dropLast(1))\n  print(s.dropLast(3))\n  print(s.dropLast(5).isEmpty)\n  print(s.dropLast(99).isEmpty)\n  print(s.dropFirst(1).dropLast(1))\n}";
    assert_eq!(
        run(source),
        "true\néあ😀z\n😀z\ntrue\ntrue\ntrue\nhéあ😀\nhé\ntrue\ntrue\néあ😀\n"
    );
}

#[test]
fn the_pieces_of_a_string_are_new_strings_that_join_back_together() {
    let source = "func main() {\n  let s = \"ab\" + \"cdéf\"\n  var n = 0\n  while n <= 6 {\n    if s.prefix(n) + s.dropFirst(n) == s { print(n) }\n    if s.dropLast(n) + s.suffix(n) == s { print(n) }\n    n += 1\n  }\n}";
    assert_eq!(run(source), "0\n0\n1\n1\n2\n2\n3\n3\n4\n4\n5\n5\n6\n6\n");
}

#[test]
fn cutting_strings_in_a_loop_does_not_leak() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    let s = \"item\" + String(i)\n    total += s.prefix(2).count + s.dropFirst(2).count + s.suffix(3).count + s.dropLast(3).count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "337780\n");
}

#[test]
fn a_negative_count_traps_and_a_wrong_argument_is_rejected() {
    run_expecting_a_crash("func main() {\n  let n = -1\n  print(\"abc\".prefix(n))\n}");
    run_expecting_a_crash("func main() {\n  let n = -1\n  print(\"abc\".dropLast(n))\n}");
    assert!(build_error("func main() { print(\"a\".prefix()) }").contains("prefix"));
    assert!(build_error("func main() { print(\"a\".prefix(\"b\")) }").contains("expected Int"));
    assert!(build_error("func main() { print(\"a\".dropFirst(count: 1)) }").contains("dropFirst"));
}

#[test]
fn replacing_occurrences_replaces_every_non_overlapping_match_from_the_left() {
    let source = "func main() {\n  print(\"banana\".replacingOccurrences(of: \"an\", with: \"AN\"))\n  print(\"aaa\".replacingOccurrences(of: \"aa\", with: \"b\"))\n  print(\"abc\".replacingOccurrences(of: \"x\", with: \"y\"))\n  print(\"a-b-c\".replacingOccurrences(of: \"-\", with: \"\"))\n  print(\"ab\".replacingOccurrences(of: \"b\", with: \"longer\"))\n  print(\"héllo\".replacingOccurrences(of: \"é\", with: \"e\"))\n  print(\"abc\".replacingOccurrences(of: \"abc\", with: \"z\"))\n  print(\"abc\".replacingOccurrences(of: \"\", with: \"z\"))\n  print(\"\".replacingOccurrences(of: \"a\", with: \"b\").isEmpty)\n  let s = \"x\" + \"yx\"\n  print(s.replacingOccurrences(of: \"x\", with: \"[x]\").replacingOccurrences(of: \"y\", with: \"Y\"))\n}";
    assert_eq!(
        run(source),
        "bANANa\nba\nabc\nabc\nalonger\nhello\nz\nabc\ntrue\n[x]Y[x]\n"
    );
}

#[test]
fn trimmed_removes_ascii_whitespace_from_both_ends() {
    let source = "func main() {\n  print(\"  hi \\n\\t\".trimmed())\n  print(\"   \".trimmed().isEmpty)\n  print(\"\".trimmed().isEmpty)\n  print(\"a b\".trimmed())\n  print(\" é \".trimmed())\n  print((\" x\" + \"y \").trimmed().count)\n  print(\"none\".trimmed())\n}";
    assert_eq!(run(source), "hi\ntrue\ntrue\na b\né\n2\nnone\n");
}

#[test]
fn first_index_gives_the_character_offset_of_the_first_match() {
    let source = "func main() {\n  let s = \"héllo😀l\"\n  print(s.firstIndex(of: \"h\") == 0)\n  print(s.firstIndex(of: \"l\") == 2)\n  print(s.firstIndex(of: \"😀\") == 5)\n  print(s.firstIndex(of: \"x\") == nil)\n  if let i = s.firstIndex(of: \"é\") { print(i) }\n  print(\"\".firstIndex(of: \"a\") == nil)\n  let c: Character = \"o\"\n  print(s.firstIndex(of: c) == 4)\n}";
    assert_eq!(run(source), "true\ntrue\ntrue\ntrue\n1\ntrue\ntrue\n");
}

#[test]
fn contains_accepts_a_character_as_well_as_a_string() {
    let source = "func main() {\n  let s = \"héllo\"\n  let e: Character = \"é\"\n  let z: Character = \"z\"\n  print(s.contains(e))\n  print(s.contains(z))\n  print(s.contains(\"ll\"))\n}";
    assert_eq!(run(source), "true\nfalse\ntrue\n");
}

#[test]
fn replacing_trimming_and_searching_in_a_loop_do_not_leak() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    let s = \" k\" + String(i) + \" \"\n    let t = s.trimmed().replacingOccurrences(of: \"k\", with: \"key\")\n    total += t.count\n    if let at = t.firstIndex(of: \"y\") { total += at }\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "188890\n");
}

#[test]
fn the_search_and_replace_members_check_their_arguments() {
    assert!(
        build_error("func main() { print(\"a\".replacingOccurrences(\"a\", \"b\")) }")
            .contains("replacingOccurrences")
    );
    assert!(
        build_error("func main() { print(\"a\".replacingOccurrences(of: \"a\")) }")
            .contains("replacingOccurrences")
    );
    assert!(
        build_error("func main() { print(\"a\".replacingOccurrences(of: 1, with: \"b\")) }")
            .contains("expected String")
    );
    assert!(
        build_error("func main() { print(\"a\".firstIndex(of: 1)) }")
            .contains("expected Character")
    );
    assert!(build_error("func main() { print(\"a\".firstIndex(\"a\")) }").contains("firstIndex"));
    assert!(build_error("func main() { print(\"a\".trimmed(1)) }").contains("trimmed"));
}

#[test]
fn an_array_literal_has_a_count_and_elements_read_by_index() {
    assert_eq!(
        main_printing("let a = [10, 20, 30]\nprint(a.count)\nprint(a[0] + a[2])\nprint(a[1])"),
        "3\n40\n20\n"
    );
}

#[test]
fn an_empty_array_takes_its_type_from_the_annotation() {
    assert_eq!(
        main_printing(
            "let e: [Int] = []\nprint(e.count)\nprint(e.isEmpty)\nlet f = [1]\nprint(f.isEmpty)"
        ),
        "0\ntrue\nfalse\n"
    );
}

#[test]
fn a_for_loop_visits_the_elements_in_order() {
    assert_eq!(
        main_printing(
            "let names = [\"ada\", \"grace\", \"alan\"]\nfor n in names {\n  print(n)\n}\nvar total = 0\nfor x in [1, 2, 3, 4] {\n  if x == 3 { continue }\n  total += x\n}\nprint(total)"
        ),
        "ada\ngrace\nalan\n7\n"
    );
}

#[test]
fn an_array_is_a_parameter_a_result_and_an_element_of_another_array() {
    assert_eq!(
        run(
            "func sum(_ xs: [Int]) -> Int {\n  var t = 0\n  for x in xs { t += x }\n  return t\n}\nfunc make() -> [Int] { return [4, 5, 6] }\nfunc main() {\n  print(sum(make()))\n  let grid = [[1, 2], [3, 4, 5]]\n  print(grid[1][2])\n  print(grid.count)\n}"
        ),
        "15\n5\n2\n"
    );
}

#[test]
fn reading_past_the_end_of_an_array_traps() {
    run_expecting_a_crash("func main() {\n  let a = [1, 2]\n  var i = 2\n  print(a[i])\n}");
    run_expecting_a_crash("func main() {\n  let a = [1, 2]\n  var i = -1\n  print(a[i])\n}");
}

#[test]
fn an_array_of_class_instances_releases_them_with_the_array() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  do_it()\n  print(0)\n}\nfunc do_it() {\n  let boxes = [Box(n: 1), Box(n: 2)]\n  let first = boxes[0]\n  print(first.n)\n}"
        ),
        "1\n1\n2\n0\n"
    );
}

#[test]
fn an_array_holds_values_of_a_generic_element_type() {
    assert_eq!(
        run(
            "func first<T>(_ xs: [T]) -> T { return xs[0] }\nfunc main() {\n  print(first([7, 8]))\n  print(first([\"x\", \"y\"]))\n}"
        ),
        "7\nx\n"
    );
}

#[test]
fn array_misuse_is_rejected_with_a_message() {
    assert!(build_error("func main() { let a = [] \n print(a.count) }").contains("empty array"));
    assert!(build_error("func main() { let a = [1, \"x\"] }").contains("expected Int"));
    assert!(build_error("func main() { let a = [1]\n print(a[true]) }").contains("expected Int"));
    assert!(
        build_error("func main() { let a = 5\n print(a[0]) }").contains("cannot be subscripted")
    );
    assert!(build_error("func main() { let a = [1]\n a.count = 2 }").contains("count"));
}

#[test]
fn arrays_built_and_dropped_in_a_loop_keep_their_contents_intact() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 200000 {\n    let a = [String(i), \"k\", String(i + 1)]\n    let b = [a, a]\n    total += b[1][2].count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "1088895\n");
}
#[test]
fn append_adds_an_element_at_the_end() {
    assert_eq!(
        main_printing(
            "var a = [1]\na.append(2)\na.append(3)\nprint(a.count)\nprint(a[0] + a[1] + a[2])\nvar e: [String] = []\ne.append(\"x\")\nprint(e[0])"
        ),
        "3\n6\nx\n"
    );
}

#[test]
fn assigning_an_element_replaces_it() {
    assert_eq!(
        main_printing(
            "var a = [1, 2, 3]\na[1] = 20\na[2] += 5\nprint(a[0])\nprint(a[1])\nprint(a[2])"
        ),
        "1\n20\n8\n"
    );
}

#[test]
fn a_copy_of_an_array_is_independent_of_the_original() {
    assert_eq!(
        main_printing(
            "var a = [1, 2]\nlet b = a\na.append(3)\na[0] = 9\nprint(a.count)\nprint(b.count)\nprint(a[0])\nprint(b[0])"
        ),
        "3\n2\n9\n1\n"
    );
}

#[test]
fn an_array_passed_to_a_function_is_not_changed_by_the_callee() {
    assert_eq!(
        run(
            "func grow(_ xs: [Int]) -> [Int] {\n  var ys = xs\n  ys.append(100)\n  return ys\n}\nfunc main() {\n  let a = [1]\n  let b = grow(a)\n  print(a.count)\n  print(b.count)\n}"
        ),
        "1\n2\n"
    );
}

#[test]
fn an_array_grows_past_its_first_capacity() {
    assert_eq!(
        main_printing(
            "var a: [Int] = []\nvar i = 0\nwhile i < 1000 {\n  a.append(i)\n  i += 1\n}\nvar total = 0\nfor x in a { total += x }\nprint(total)\nprint(a.count)"
        ),
        "499500\n1000\n"
    );
}

#[test]
fn an_array_member_of_a_struct_or_class_can_be_appended_to() {
    assert_eq!(
        run(
            "struct Bag {\n  var items: [Int]\n}\nclass Log {\n  var lines: [String]\n  init() { self.lines = [] }\n  func add(_ s: String) { lines.append(s) }\n}\nfunc main() {\n  var bag = Bag(items: [1])\n  bag.items.append(2)\n  let copy = bag\n  bag.items.append(3)\n  print(bag.items.count)\n  print(copy.items.count)\n  let log = Log()\n  log.add(\"a\")\n  log.add(\"b\")\n  print(log.lines.count)\n}"
        ),
        "3\n2\n2\n"
    );
}

#[test]
fn replacing_and_appending_class_elements_runs_deinit_when_they_are_dropped() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  var a = [Box(n: 1)]\n  a.append(Box(n: 2))\n  a[0] = Box(n: 3)\n  print(0)\n}"
        ),
        "1\n0\n3\n2\n"
    );
}

#[test]
fn a_loop_variable_copy_keeps_the_original_array_while_it_is_modified() {
    assert_eq!(
        main_printing(
            "var a = [1, 2, 3]\nvar seen = 0\nfor x in a {\n  a.append(x)\n  seen += 1\n}\nprint(seen)\nprint(a.count)"
        ),
        "3\n6\n"
    );
}

#[test]
fn appending_the_wrong_type_or_to_a_constant_is_rejected() {
    assert!(build_error("func main() { var a = [1]\n a.append(\"x\") }").contains("expected Int"));
    assert!(build_error("func main() { let a = [1]\n a.append(2) }").contains("constant"));
    assert!(build_error("func main() { var a = [1]\n a.append() }").contains("append"));
    assert!(build_error("func main() { let a = [1]\n a[0] = 2 }").contains("constant"));
    assert!(build_error("func main() { var a = [1]\n a[0] = true }").contains("expected Int"));
}

#[test]
fn a_generic_function_can_append_to_an_array_of_its_type_parameter() {
    assert_eq!(
        run(
            "func pushed<T>(_ xs: [T], _ x: T) -> [T] {\n  var ys = xs\n  ys.append(x)\n  ys[0] = x\n  return ys\n}\nfunc main() {\n  let a = pushed([1, 2], 9)\n  print(a[0])\n  print(a.count)\n  let b = pushed([\"p\"], \"q\")\n  print(b[1])\n}"
        ),
        "9\n3\nq\n"
    );
}

#[test]
fn only_a_variable_or_a_member_can_be_the_array_that_is_changed() {
    assert!(build_error("func main() { [1][0] = 2 }").contains("can be changed"));
    assert!(build_error("func main() { var a = [1]\n a.append(x: 2) }").contains("append"));
    assert!(build_error("func main() { var a = [1]\n a.append(1, 2) }").contains("append"));
    assert!(build_error("func main() { var n = 5\n n[0] = 1 }").contains("not an array"));
}

#[test]
fn an_array_holds_structs_and_updates_them_in_place() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var y: Int\n}\nfunc main() {\n  var ps = [P(x: 1, y: 2)]\n  ps.append(P(x: 3, y: 4))\n  ps.append(ps[0])\n  ps[0] = P(x: 9, y: 8)\n  ps[1] = ps[2]\n  let copy = ps\n  ps.append(P(x: 7, y: 7))\n  var sum = 0\n  for p in ps { sum += p.x * 10 + p.y }\n  print(sum)\n  print(ps[0].x)\n  print(ps[1].y)\n  print(copy.count)\n  print(ps.count)\n}"
        ),
        "199\n9\n2\n3\n4\n"
    );
}

#[test]
fn an_array_holds_optionals_and_enums_with_payloads() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case square(Int)\n}\nfunc main() {\n  var a: [Int?] = [1, nil]\n  a.append(3)\n  a[1] = 2\n  var total = 0\n  for x in a {\n    if let v = x { total += v }\n  }\n  print(total)\n  var names: [String?] = []\n  names.append(\"n\")\n  names.append(nil)\n  print(names[0] ?? \"-\")\n  print(names[1] ?? \"-\")\n  var shapes = [Shape.circle(1)]\n  shapes.append(.square(5))\n  switch shapes[1] {\n  case .circle(let r): print(r)\n  case .square(let s): print(s * 2)\n  }\n}"
        ),
        "6\nn\n-\n10\n"
    );
}

#[test]
fn structs_with_counted_members_in_an_array_are_released_with_it() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nstruct H {\n  var box: Box\n  var tag: Int\n}\nfunc main() {\n  var hs = [H(box: Box(n: 1), tag: 1)]\n  hs.append(H(box: Box(n: 2), tag: 2))\n  let copy = hs\n  hs[0] = H(box: Box(n: 3), tag: 3)\n  print(0)\n  var os: [Box?] = [Box(n: 4)]\n  os[0] = nil\n  print(5)\n}"
        ),
        "0\n4\n5\n1\n3\n2\n"
    );
}

#[test]
fn arrays_of_structs_built_and_dropped_in_a_loop_keep_their_contents() {
    let source = "struct P {\n  var s: String\n  var n: Int\n}\nfunc main() {\n  var total = 0\n  var i = 0\n  while i < 100000 {\n    var a = [P(s: String(i), n: 1)]\n    a.append(P(s: \"k\", n: 2))\n    let b = a\n    a[0] = P(s: \"z\", n: 3)\n    total += b[0].s.count + a[0].n + b[1].n\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "988890\n");
}

#[test]
fn first_and_last_are_optionals_that_are_nil_for_an_empty_array() {
    assert_eq!(
        main_printing(
            "let a = [4, 5, 6]\nprint(a.first ?? -1)\nprint(a.last ?? -1)\nlet e: [Int] = []\nprint(e.first ?? -1)\nprint(e.last == nil)\nlet s = [\"x\", \"y\"]\nif let l = s.last { print(l) }"
        ),
        "4\n6\n-1\ntrue\ny\n"
    );
}

#[test]
fn removing_the_last_element_shrinks_the_array_and_traps_when_empty() {
    assert_eq!(
        main_printing(
            "var a = [1, 2, 3]\nlet b = a\na.removeLast()\nprint(a.count)\nprint(b.count)\nprint(a.last ?? 0)\na.removeLast()\na.removeLast()\nprint(a.isEmpty)"
        ),
        "2\n3\n2\ntrue\n"
    );
    run_expecting_a_crash("func main() {\n  var a: [Int] = []\n  a.removeLast()\n}");
}

#[test]
fn removed_class_elements_run_deinit() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  var a = [Box(n: 1), Box(n: 2)]\n  a.removeLast()\n  print(0)\n}"
        ),
        "2\n0\n1\n"
    );
}

#[test]
fn insert_places_an_element_at_a_position_and_shifts_the_rest() {
    assert_eq!(
        main_printing(
            "var a = [1, 3]\na.insert(2, at: 1)\na.insert(0, at: 0)\na.insert(4, at: 4)\nvar t = 0\nfor x in a { t = t * 10 + x }\nprint(t)\nprint(a.count)"
        ),
        "1234\n5\n"
    );
    run_expecting_a_crash("func main() {\n  var a = [1]\n  a.insert(2, at: 3)\n}");
}

#[test]
fn insert_works_for_structs_strings_and_a_shared_array() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var y: Int\n}\nfunc main() {\n  var ps = [P(x: 1, y: 1), P(x: 3, y: 3)]\n  let old = ps\n  ps.insert(P(x: 2, y: 2), at: 1)\n  print(ps[1].x)\n  print(old.count)\n  var ss = [\"a\"]\n  ss.insert(\"b\", at: 0)\n  print(ss[0] + ss[1])\n}"
        ),
        "2\n2\nba\n"
    );
}

#[test]
fn arrays_are_concatenated_with_plus_and_plus_equals() {
    assert_eq!(
        main_printing(
            "let a = [1, 2]\nlet b = a + [3]\nvar c = b + []\nc += a\nprint(b.count)\nprint(c.count)\nprint(c[4])\nprint(a.count)\nlet s = [\"x\"] + [\"y\", \"z\"]\nprint(s[2])"
        ),
        "3\n5\n2\n2\nz\n"
    );
}

#[test]
fn arrays_compare_by_length_and_elements() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nfunc main() {\n  print([1, 2] == [1, 2])\n  print([1, 2] != [1, 3])\n  print([1] == [1, 2])\n  print([\"a\"] == [\"a\"])\n  print([[1], [2]] == [[1], [2]])\n  let e: [Int] = []\n  print(e == [])\n  print([P(x: 1)] == [P(x: 1)])\n  let n: [Int?] = [nil]\n  print(n == [nil])\n}"
        ),
        "true\ntrue\nfalse\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn contains_searches_by_equality() {
    assert_eq!(
        main_printing(
            "let a = [1, 2, 3]\nprint(a.contains(2))\nprint(a.contains(5))\nlet s = [\"x\", \"y\"]\nprint(s.contains(\"y\"))\nlet n: [Int?] = [1, nil]\nprint(n.contains(nil))\nlet g = [[1], [2]]\nprint(g.contains([2]))"
        ),
        "true\nfalse\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn array_equality_and_contains_work_in_generic_code() {
    assert_eq!(
        run(
            "func has<T: Equatable>(_ xs: [T], _ x: T) -> Bool { return xs.contains(x) }\nfunc same<T: Equatable>(_ a: [T], _ b: [T]) -> Bool { return a == b }\nfunc main() {\n  print(has([1, 2], 2))\n  print(has([\"a\"], \"b\"))\n  print(same([1], [1]))\n  print(same([[1]], [[2]]))\n}"
        ),
        "true\nfalse\ntrue\nfalse\n"
    );
}

#[test]
fn the_new_array_members_reject_misuse() {
    assert!(
        build_error("class C {}\nfunc main() { let a = [C()]\n print(a.contains(C())) }")
            .contains("equality")
    );
    assert!(build_error("func main() { let a = [1]\n print(a.contains()) }").contains("contains"));
    assert!(
        build_error("func main() { let a = [1]\n print(a.contains(x: 1)) }").contains("contains")
    );
    assert!(
        build_error("func main() { let a = [1]\n let b = a.removeLast() }").contains("constant")
    );
    assert!(build_error("func main() { var a = [1]\n let b = a.append(1) }").contains("statement"));
    assert!(build_error("func main() { var a = [1]\n a.removeLast(1) }").contains("removeLast"));
    assert!(build_error("func main() { var a = [1]\n a.insert(1) }").contains("insert"));
    assert!(build_error("func main() { var a = [1]\n a.insert(1, 0) }").contains("insert"));
    assert!(build_error("func main() { let a = [1]\n print(a.shuffled()) }").contains("no method"));
    assert!(build_error("func main() { let a = [1] + [\"x\"] }").contains("expected"));
    assert!(build_error("func main() { print([1] == [\"x\"]) }").contains("expected"));
    assert!(build_error("func main() { let a = [1]\n a.first = 2 }").contains("no member"));
}

#[test]
fn remove_at_deletes_an_element_and_closes_the_gap() {
    assert_eq!(
        main_printing(
            "var a = [1, 2, 3, 4]\nlet b = a\na.remove(at: 1)\na.remove(at: 2)\na.remove(at: 0)\nprint(a.count)\nprint(a[0])\nprint(b.count)\nvar t = 0\nfor x in b { t = t * 10 + x }\nprint(t)"
        ),
        "1\n3\n4\n1234\n"
    );
    run_expecting_a_crash("func main() {\n  var a = [1]\n  a.remove(at: 1)\n}");
    run_expecting_a_crash("func main() {\n  var a = [1]\n  a.remove(at: -1)\n}");
}

#[test]
fn remove_at_releases_the_removed_element_and_handles_structs() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nstruct H {\n  var box: Box\n  var tag: Int\n}\nfunc main() {\n  var bs = [Box(n: 1), Box(n: 2), Box(n: 3)]\n  bs.remove(at: 1)\n  print(0)\n  var hs = [H(box: Box(n: 4), tag: 4), H(box: Box(n: 5), tag: 5)]\n  hs.remove(at: 0)\n  print(hs[0].tag)\n  var s = [\"a\", \"b\", \"c\"]\n  s.remove(at: 0)\n  print(s[0] + s[1])\n}"
        ),
        "2\n0\n4\n5\nbc\n5\n1\n3\n"
    );
}

#[test]
fn remove_at_checks_its_arguments_and_is_a_statement() {
    assert!(build_error("func main() { var a = [1]\n a.remove(0) }").contains("remove"));
    assert!(build_error("func main() { var a = [1]\n a.remove(at: 0, 1) }").contains("remove"));
    assert!(
        build_error("func main() { let a = [1]\n let x = a.remove(at: 0) }").contains("constant")
    );
    assert!(build_error("func main() { let a = [1]\n a.remove(at: 0) }").contains("constant"));
}

#[test]
fn an_element_of_an_element_can_be_assigned_and_changed() {
    assert_eq!(
        main_printing(
            "var g = [[1, 2], [3]]\ng[0][1] = 20\ng[1][0] += 5\ng[1].append(9)\ng[0].insert(7, at: 0)\ng[0].remove(at: 1)\ng[1].removeLast()\nprint(g[0][0])\nprint(g[0][1])\nprint(g[0].count)\nprint(g[1][0])\nprint(g[1].count)"
        ),
        "7\n20\n2\n8\n1\n"
    );
}

#[test]
fn changing_an_element_of_an_element_does_not_touch_a_copy() {
    assert_eq!(
        main_printing(
            "var g = [[1, 2], [3, 4]]\nlet h = g\ng[0][0] = 100\ng[1].append(5)\nprint(g[0][0])\nprint(h[0][0])\nprint(g[1].count)\nprint(h[1].count)\nvar k = g\nk[0][1] = 7\nprint(g[0][1])\nprint(k[0][1])"
        ),
        "100\n1\n3\n2\n2\n7\n"
    );
}

#[test]
fn a_row_shared_between_two_arrays_is_copied_when_one_changes() {
    assert_eq!(
        main_printing(
            "let row = [1, 2]\nvar g = [row, row]\ng[0][0] = 9\nprint(g[0][0])\nprint(g[1][0])\nprint(row[0])"
        ),
        "9\n1\n1\n"
    );
}

#[test]
fn deeper_and_member_paths_of_elements_can_be_changed() {
    assert_eq!(
        run(
            "class Board {\n  var cells: [[Int]]\n  init() { self.cells = [[0, 0], [0, 0]] }\n  func set(_ r: Int, _ c: Int, _ v: Int) { cells[r][c] = v }\n}\nstruct Bag {\n  var rows: [[String]]\n}\nfunc main() {\n  let b = Board()\n  b.set(1, 0, 5)\n  print(b.cells[1][0])\n  var bag = Bag(rows: [[\"a\"]])\n  bag.rows[0][0] += \"b\"\n  bag.rows[0].append(\"c\")\n  print(bag.rows[0][0] + bag.rows[0][1])\n  var cube = [[[1]]]\n  cube[0][0][0] = 4\n  cube[0][0].append(5)\n  print(cube[0][0][0] + cube[0][0][1])\n}"
        ),
        "5\nabc\n9\n"
    );
}

#[test]
fn nested_updates_release_replaced_elements_and_trap_out_of_range() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  var g = [[Box(n: 1)], [Box(n: 2)]]\n  g[0][0] = Box(n: 3)\n  print(0)\n}"
        ),
        "1\n0\n3\n2\n"
    );
    run_expecting_a_crash("func main() {\n  var g = [[1]]\n  g[0][1] = 2\n}");
    run_expecting_a_crash("func main() {\n  var g = [[1]]\n  g[1][0] = 2\n}");
}

#[test]
fn an_element_of_a_constant_array_cannot_be_changed() {
    assert!(build_error("func main() { let g = [[1]]\n g[0][0] = 2 }").contains("constant"));
    assert!(build_error("func main() { var g = [1]\n g[0][0] = 2 }").contains("not an array"));
}

#[test]
fn remove_last_and_remove_at_produce_the_removed_element() {
    assert_eq!(
        main_printing(
            "var a = [10, 20, 30, 40]\nlet b = a\nlet last = a.removeLast()\nlet mid = a.remove(at: 1)\nprint(last)\nprint(mid)\nprint(a.count)\nprint(b.count)\nprint(a.removeLast() + a.removeLast())\nprint(a.isEmpty)"
        ),
        "40\n20\n2\n4\n40\ntrue\n"
    );
}

#[test]
fn a_removed_element_keeps_its_instance_alive_until_it_is_dropped() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nstruct H {\n  var box: Box\n  var tag: Int\n}\nfunc main() {\n  var bs = [Box(n: 1), Box(n: 2)]\n  let taken = bs.removeLast()\n  print(0)\n  print(taken.n)\n  var hs = [H(box: Box(n: 3), tag: 3), H(box: Box(n: 4), tag: 4)]\n  let h = hs.remove(at: 0)\n  print(h.tag)\n  print(hs[0].tag)\n  var ss = [\"a\", \"b\"]\n  print(ss.removeLast() + ss.removeLast())\n}"
        ),
        "0\n2\n3\n4\nba\n3\n4\n2\n1\n"
    );
}

#[test]
fn pop_last_returns_nil_for_an_empty_array_and_drains_in_a_loop() {
    assert_eq!(
        main_printing(
            "var a = [1, 2, 3]\nvar order = 0\nwhile let x = a.popLast() {\n  order = order * 10 + x\n}\nprint(order)\nprint(a.popLast() == nil)\nvar s = [\"p\"]\nif let v = s.popLast() { print(v) }\nprint(s.count)\nvar e: [Int] = []\ne.popLast()\nprint(e.count)"
        ),
        "321\ntrue\np\n0\n0\n"
    );
}

#[test]
fn taking_from_an_empty_array_traps_and_a_shared_array_is_not_changed() {
    run_expecting_a_crash(
        "func main() {\n  var a: [Int] = []\n  let x = a.removeLast()\n  print(x)\n}",
    );
    run_expecting_a_crash("func main() {\n  var a = [1]\n  let x = a.remove(at: 1)\n  print(x)\n}");
    assert_eq!(
        main_printing(
            "var g = [[1, 2], [3, 4]]\nlet h = g\nlet row = g[0].removeLast()\nlet gone = g.remove(at: 1)\nprint(row)\nprint(gone[1])\nprint(g.count)\nprint(g[0].count)\nprint(h[0].count)\nprint(h.count)"
        ),
        "2\n4\n1\n1\n2\n2\n"
    );
}

#[test]
fn remove_all_empties_an_array_without_touching_a_copy() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  var a = [1, 2, 3]\n  let b = a\n  a.removeAll()\n  print(a.count)\n  print(b.count)\n  a.append(9)\n  print(a[0])\n  var bs = [Box(n: 1)]\n  bs.removeAll()\n  print(0)\n  var g = [[1], [2]]\n  g[0].removeAll()\n  print(g[0].count + g[1].count)\n}"
        ),
        "0\n3\n9\n1\n0\n1\n"
    );
}

#[test]
fn the_taking_members_check_their_arguments() {
    assert!(build_error("func main() { var a = [1]\n let x = a.popLast(1) }").contains("popLast"));
    assert!(
        build_error("func main() { var a = [1]\n let x = a.removeLast(1) }").contains("removeLast")
    );
    assert!(build_error("func main() { var a = [1]\n let x = a.remove(0) }").contains("remove"));
    assert!(build_error("func main() { var a = [1]\n a.removeAll(1) }").contains("removeAll"));
    assert!(
        build_error("func main() { var a = [1]\n let x = a.removeAll() }").contains("statement")
    );
    assert!(build_error("func main() { let a = [1]\n a.removeAll() }").contains("constant"));
}

#[test]
fn a_member_of_a_struct_element_can_be_assigned() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var y: Int\n}\nfunc main() {\n  var ps = [P(x: 1, y: 2), P(x: 3, y: 4)]\n  ps[0].x = 10\n  ps[1].y += 5\n  ps[1].x *= 2\n  print(ps[0].x)\n  print(ps[0].y)\n  print(ps[1].x)\n  print(ps[1].y)\n}"
        ),
        "10\n2\n6\n9\n"
    );
}

#[test]
fn assigning_a_member_of_an_element_does_not_touch_a_copy() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nfunc main() {\n  var ps = [P(x: 1), P(x: 2)]\n  let copy = ps\n  ps[0].x = 9\n  print(ps[0].x)\n  print(copy[0].x)\n  var other = ps\n  other[1].x = 7\n  print(ps[1].x)\n  print(other[1].x)\n}"
        ),
        "9\n1\n2\n7\n"
    );
}

#[test]
fn members_of_members_arrays_and_classes_under_an_element_can_be_changed() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nstruct Inner {\n  var v: Int\n}\nstruct Outer {\n  var inner: Inner\n  var items: [Int]\n  var box: Box\n  var name: String\n}\nfunc main() {\n  var os = [Outer(inner: Inner(v: 1), items: [1], box: Box(n: 1), name: \"a\")]\n  os[0].inner.v = 5\n  os[0].items.append(2)\n  os[0].items[0] = 8\n  os[0].name += \"b\"\n  os[0].box = Box(n: 2)\n  os[0].box.n = 3\n  print(os[0].inner.v)\n  print(os[0].items[0] + os[0].items[1])\n  print(os[0].name)\n  print(os[0].box.n)\n}"
        ),
        "1\n5\n10\nab\n3\n3\n"
    );
}

#[test]
fn a_member_of_an_element_of_a_nested_array_and_of_a_class_element_can_be_assigned() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nclass C {\n  var n: Int\n}\nfunc main() {\n  var g = [[P(x: 1)], [P(x: 2)]]\n  let h = g\n  g[1][0].x = 20\n  print(g[1][0].x)\n  print(h[1][0].x)\n  let cs = [C(n: 1)]\n  cs[0].n = 4\n  print(cs[0].n)\n}"
        ),
        "20\n2\n4\n"
    );
}

#[test]
fn assigning_a_member_of_a_struct_element_traps_out_of_range_and_checks_constants() {
    run_expecting_a_crash(
        "struct P {\n  var x: Int\n}\nfunc main() {\n  var ps = [P(x: 1)]\n  ps[1].x = 2\n}",
    );
    assert!(
        build_error(
            "struct P {\n  var x: Int\n}\nfunc main() {\n  let ps = [P(x: 1)]\n  ps[0].x = 2\n}"
        )
        .contains("constant")
    );
    assert!(
        build_error(
            "struct P {\n  let x: Int\n}\nfunc main() {\n  var ps = [P(x: 1)]\n  ps[0].x = 2\n}"
        )
        .contains("constant")
    );
    assert!(
        build_error(
            "struct P {\n  var x: Int\n}\nfunc main() {\n  var ps = [P(x: 1)]\n  ps[0].z = 2\n}"
        )
        .contains("no member")
    );
}

#[test]
fn split_cuts_a_string_at_a_separator_and_skips_empty_pieces() {
    assert_eq!(
        main_printing(
            "let parts = \"a,b,,c\".split(separator: \",\")\nprint(parts.count)\nprint(parts[0] + parts[1] + parts[2])\nlet words = \"  one two  three \".split(separator: \" \")\nprint(words.count)\nprint(words[2])\nlet c: Character = \"-\"\nprint(\"x-y\".split(separator: c).count)\nprint(\"abc\".split(separator: \"abc\").count)\nprint(\"\".split(separator: \",\").count)\nprint(\"a::b\".split(separator: \"::\").count)\nprint(\"abc\".split(separator: \"\").count)"
        ),
        "3\nabc\n3\nthree\n2\n0\n0\n2\n1\n"
    );
}

#[test]
fn split_handles_multibyte_text_and_returns_independent_strings() {
    assert_eq!(
        main_printing(
            "let parts = \"héllo→wörld→!\".split(separator: \"→\")\nprint(parts[0])\nprint(parts[1])\nprint(parts[2])\nvar total = 0\nfor p in parts { total += p.count }\nprint(total)"
        ),
        "héllo\nwörld\n!\n13\n"
    );
}

#[test]
fn joined_concatenates_strings_with_a_separator() {
    assert_eq!(
        main_printing(
            "let a = [\"x\", \"y\", \"z\"]\nprint(a.joined(separator: \", \"))\nprint(a.joined())\nlet e: [String] = []\nprint(e.joined(separator: \"-\").isEmpty)\nprint(\"1 2 3\".split(separator: \" \").joined(separator: \"+\"))\nprint([\"solo\"].joined(separator: \"/\"))"
        ),
        "x, y, z\nxyz\ntrue\n1+2+3\nsolo\n"
    );
}

#[test]
fn a_string_is_subscripted_by_scalar_offset_and_by_range() {
    assert_eq!(
        main_printing(
            "let s = \"héllo\"\nprint(s[0])\nprint(s[1])\nprint(s[4])\nprint(s[1..<3])\nprint(s[0...1])\nprint(s[2..<2].isEmpty)\nprint(s[0..<5] == s)\nvar i = 3\nprint(s[i])"
        ),
        "h\né\no\nél\nhé\ntrue\ntrue\nl\n"
    );
}

#[test]
fn a_string_subscript_outside_the_string_traps() {
    for access in ["s[5]", "s[-1]", "s[2..<9]", "s[3..<2]", "s[-1..<2]"] {
        run_expecting_a_crash(&format!(
            "func main() {{\n  let s = \"héllo\"\n  var a = 0\n  a += 0\n  print({access})\n}}"
        ));
    }
}

#[test]
fn the_new_string_members_check_their_arguments() {
    assert!(build_error("func main() { print(\"a\".split(\",\")) }").contains("split"));
    assert!(
        build_error("func main() { print(\"a\".split(separator: 1)) }").contains("expected String")
    );
    assert!(build_error("func main() { print([1].joined()) }").contains("joined"));
    assert!(build_error("func main() { print([\"a\"].joined(\",\")) }").contains("joined"));
    assert!(
        build_error("func main() { print([\"a\"].joined(separator: 1)) }")
            .contains("expected String")
    );
    assert!(build_error("func main() { print(\"a\"[true]) }").contains("expected Int"));
    assert!(
        build_error("func main() {\n var s = \"ab\"\n s[0] = \"x\"\n}").contains("not an array")
    );
}

#[test]
fn splitting_joining_and_slicing_in_a_loop_keep_their_results() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 50000 {\n    let line = \"k\" + String(i) + \",v,\" + String(i + 1)\n    let parts = line.split(separator: \",\")\n    let back = parts.joined(separator: \";\")\n    total += back.count + parts[1].count + back[0..<2].count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "827784\n");
}

#[test]
fn a_dictionary_literal_is_read_by_key_and_counted() {
    assert_eq!(
        main_printing(
            "let d = [\"a\": 1, \"b\": 2, \"c\": 3]\nprint(d.count)\nprint(d[\"b\"] ?? -1)\nprint(d[\"z\"] ?? -1)\nprint(d[\"z\"] == nil)\nprint(d[\"z\", default: 9])\nprint(d[\"a\", default: 9])\nprint(d.isEmpty)\nlet e: [Int: String] = [:]\nprint(e.isEmpty)\nprint(e.count)"
        ),
        "3\n2\n-1\ntrue\n9\n1\nfalse\ntrue\n0\n"
    );
}

#[test]
fn assigning_to_a_key_inserts_replaces_and_nil_removes() {
    assert_eq!(
        main_printing(
            "var d: [String: Int] = [:]\nd[\"x\"] = 1\nd[\"y\"] = 2\nd[\"x\"] = 10\nprint(d.count)\nprint(d[\"x\"] ?? 0)\nd[\"y\"] = nil\nprint(d.count)\nprint(d[\"y\"] == nil)\nd[\"missing\"] = nil\nprint(d.count)\nd[\"n\", default: 0] += 5\nd[\"n\", default: 0] += 5\nprint(d[\"n\"] ?? 0)"
        ),
        "2\n10\n1\ntrue\n1\n10\n"
    );
}

#[test]
fn a_dictionary_iterates_in_insertion_order() {
    assert_eq!(
        main_printing(
            "var d: [Int: String] = [:]\nd[30] = \"c\"\nd[10] = \"a\"\nd[20] = \"b\"\nd[10] = \"A\"\nd[30] = nil\nd[40] = \"d\"\nfor (k, v) in d {\n  print(\"\\(k)=\\(v)\")\n}\nprint(d.keys.count)\nprint(d.values[0] + d.values[2])"
        ),
        "10=A\n20=b\n40=d\n3\nAd\n"
    );
}

#[test]
fn a_dictionary_copy_is_independent_of_the_original() {
    assert_eq!(
        main_printing(
            "var a = [1: 1, 2: 2]\nlet b = a\na[3] = 3\na[1] = 100\na[2] = nil\nprint(a.count)\nprint(b.count)\nprint(a[1] ?? 0)\nprint(b[1] ?? 0)\nprint(b[2] ?? 0)\nvar c = b\nc.removeAll()\nprint(c.count)\nprint(b.count)"
        ),
        "2\n2\n100\n1\n2\n0\n2\n"
    );
}

#[test]
fn a_dictionary_grows_and_survives_many_removals() {
    assert_eq!(
        main_printing(
            "var d: [Int: Int] = [:]\nvar i = 0\nwhile i < 2000 {\n  d[i] = i * 2\n  i += 1\n}\ni = 0\nwhile i < 2000 {\n  if i % 3 != 0 { d[i] = nil }\n  i += 1\n}\nvar total = 0\nfor (k, v) in d { total += k + v }\nprint(d.count)\nprint(total)\nprint(d[3] ?? -1)\nprint(d[4] ?? -1)\nd[4] = 8\nprint(d[4] ?? -1)"
        ),
        "667\n1998999\n6\n-1\n8\n"
    );
}

#[test]
fn removing_and_updating_values_return_the_old_one() {
    assert_eq!(
        main_printing(
            "var d = [\"a\": 1, \"b\": 2]\nprint(d.removeValue(forKey: \"a\") ?? -1)\nprint(d.removeValue(forKey: \"a\") ?? -1)\nprint(d.updateValue(5, forKey: \"b\") ?? -1)\nprint(d.updateValue(6, forKey: \"c\") ?? -1)\nprint(d.count)\nprint(d[\"b\"] ?? 0)\nd.removeValue(forKey: \"b\")\nprint(d.count)"
        ),
        "1\n-1\n2\n-1\n2\n5\n1\n"
    );
}

#[test]
fn dictionaries_hold_class_instances_and_release_them() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  var d = [\"a\": Box(n: 1), \"b\": Box(n: 2)]\n  d[\"a\"] = Box(n: 3)\n  print(0)\n  d[\"b\"] = nil\n  print(9)\n}"
        ),
        "1\n0\n2\n9\n3\n"
    );
}

#[test]
fn a_dictionary_key_can_be_a_character_a_bool_an_enum_a_struct_an_array_or_an_optional() {
    assert_eq!(
        run(
            "enum Color {\n  case red\n  case green\n}\nstruct P {\n  var x: Int\n  var name: String\n}\nfunc main() {\n  var c: [Character: Int] = [\"a\": 1]\n  c[\"b\"] = 2\n  print(c[\"b\"] ?? 0)\n  var b = [true: \"yes\", false: \"no\"]\n  print(b[false] ?? \"\")\n  var e = [Color.red: 1, Color.green: 2]\n  e[.red] = 10\n  print(e[.red] ?? 0)\n  var s: [P: Int] = [P(x: 1, name: \"a\"): 5]\n  s[P(x: 1, name: \"a\")] = 6\n  s[P(x: 2, name: \"a\")] = 7\n  print(s.count)\n  print(s[P(x: 1, name: \"a\")] ?? 0)\n  var a: [[Int]: String] = [[1, 2]: \"p\"]\n  a[[1, 2]] = \"q\"\n  a[[2, 1]] = \"r\"\n  print(a.count)\n  print(a[[1, 2]] ?? \"\")\n  var o: [Int?: Int] = [nil: 1, 5: 2]\n  o[nil] = 3\n  print(o[nil] ?? 0)\n  print(o[5] ?? 0)\n  print(o.count)\n}"
        ),
        "2\nno\n10\n2\n6\n2\nq\n3\n2\n2\n"
    );
}

#[test]
fn dictionary_values_can_be_structs_arrays_and_dictionaries() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var y: Int\n}\nfunc main() {\n  var ps: [String: P] = [\"a\": P(x: 1, y: 2)]\n  ps[\"b\"] = P(x: 3, y: 4)\n  print((ps[\"b\"] ?? P(x: 0, y: 0)).y)\n  var arrays = [\"k\": [1, 2]]\n  arrays[\"k\"] = (arrays[\"k\"] ?? []) + [3]\n  print(arrays[\"k\"]?.count ?? 0)\n  var nested: [Int: [String: Int]] = [1: [\"a\": 1]]\n  nested[1] = [\"b\": 2]\n  print((nested[1] ?? [:])[\"b\"] ?? 0)\n  var total = 0\n  for (_k, p) in ps { total += p.x + p.y }\n  print(total)\n}"
        ),
        "4\n3\n2\n10\n"
    );
}

#[test]
fn struct_and_class_members_hold_dictionaries_that_can_change() {
    assert_eq!(
        run(
            "struct Index {\n  var counts: [String: Int]\n}\nclass Cache {\n  var store: [Int: String]\n  init() { self.store = [:] }\n  func put(_ k: Int, _ v: String) { store[k] = v }\n  func size() -> Int { return store.count }\n}\nfunc main() {\n  var i = Index(counts: [:])\n  i.counts[\"a\", default: 0] += 1\n  i.counts[\"a\", default: 0] += 1\n  let copy = i\n  i.counts[\"b\"] = 1\n  print(i.counts[\"a\"] ?? 0)\n  print(i.counts.count)\n  print(copy.counts.count)\n  let c = Cache()\n  c.put(1, \"x\")\n  c.put(2, \"y\")\n  c.put(1, \"z\")\n  print(c.size())\n  print(c.store[1] ?? \"\")\n}"
        ),
        "2\n2\n1\n2\nz\n"
    );
}

#[test]
fn a_generic_function_can_use_a_dictionary_of_its_type_parameters() {
    assert_eq!(
        run(
            "func get<K, V>(_ d: [K: V], _ k: K) -> V? { return d[k] }\nfunc names<K, V>(_ d: [K: V]) -> [K] { return d.keys }\nfunc put<K, V>(_ d: [K: V], _ k: K, _ v: V) -> [K: V] {\n  var copy = d\n  copy[k] = v\n  return copy\n}\nstruct P {\n  var x: Int\n}\nfunc main() {\n  print(get([\"a\": 4], \"a\") ?? -1)\n  print(get([1: \"x\"], 2) ?? \"none\")\n  print(get([P(x: 1): 7], P(x: 1)) ?? -1)\n  print(names([3: 1, 4: 2]).count)\n  let d = put([\"a\": 1], \"b\", 2)\n  print(d.count)\n  print(d[\"b\"] ?? 0)\n}"
        ),
        "4\nnone\n7\n2\n2\n2\n"
    );
}

#[test]
fn dictionaries_built_and_dropped_in_a_loop_keep_their_contents() {
    let source = "func main() {\n  var total = 0\n  var i = 0\n  while i < 20000 {\n    var d: [String: [Int]] = [:]\n    d[String(i)] = [i]\n    d[\"k\"] = [1, 2]\n    let e = d\n    d[String(i)] = nil\n    total += e.count + d.count + (e[String(i)] ?? [0])[0] % 2\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "70000\n");
}

#[test]
fn a_set_keeps_each_member_once_in_insertion_order() {
    assert_eq!(
        main_printing(
            "var s: Set<Int> = [3, 1, 3, 2]\nprint(s.count)\nprint(s.contains(1))\nprint(s.contains(9))\ns.insert(9)\ns.insert(1)\ns.remove(3)\nprint(s.count)\nvar order = 0\nfor x in s { order = order * 10 + x }\nprint(order)\nlet e: Set<String> = []\nprint(e.isEmpty)\nvar t: Set<String> = [\"a\"]\nlet u = t\nt.insert(\"b\")\nprint(t.count)\nprint(u.count)\nt.removeAll()\nprint(t.isEmpty)\nprint(u.contains(\"a\"))"
        ),
        "3\ntrue\nfalse\n3\n129\ntrue\n2\n1\ntrue\ntrue\n"
    );
}

#[test]
fn a_set_holds_structs_arrays_and_characters() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nfunc main() {\n  var ps: Set<P> = [P(x: 1)]\n  ps.insert(P(x: 1))\n  ps.insert(P(x: 2))\n  print(ps.count)\n  print(ps.contains(P(x: 2)))\n  var arrays: Set<[Int]> = [[1], [1], [2, 3]]\n  print(arrays.count)\n  var cs: Set<Character> = []\n  for c in \"hello\" { cs.insert(c) }\n  print(cs.count)\n  print(cs.contains(\"l\"))\n}"
        ),
        "2\ntrue\n2\n4\ntrue\n"
    );
}

#[test]
fn the_dictionary_and_set_forms_reject_misuse() {
    assert!(
        build_error("func main() { let d = [:]\n print(d.count) }").contains("empty dictionary")
    );
    assert!(
        build_error("class C {}\nfunc main() { let d: [C: Int] = [:] }")
            .contains("cannot be a key")
    );
    assert!(
        build_error("func main() { let d = [\"a\": 1]\n print(d[1]) }").contains("expected String")
    );
    assert!(build_error("func main() { let d = [\"a\": 1]\n d[\"b\"] = 2 }").contains("constant"));
    assert!(build_error("func main() { var d = [\"a\": 1]\n d[\"a\"] += 1 }").contains("default"));
    assert!(
        build_error("func main() { var d = [\"a\": 1]\n d[\"a\"] = \"x\" }")
            .contains("expected Int")
    );
    assert!(
        build_error("func main() { let d = [\"a\": 1]\n for x in d { } }").contains("(key, value)")
    );
    assert!(
        build_error("func main() { let a = [1]\n for (k, v) in a { } }").contains("dictionary")
    );
    assert!(
        build_error("func main() { var s: Set<Int> = [1]\n let x = s.insert(2) }")
            .contains("statement")
    );
    assert!(
        build_error("func main() { let d = [\"a\": 1]\n print(d.sorted()) }").contains("no method")
    );
    assert!(
        build_error("func main() { let s: Set<Int> = [1]\n print(s.keys) }").contains("member")
    );
    assert!(
        build_error("func main() { var d = [\"a\": 1]\n d.removeValue(\"a\") }")
            .contains("removeValue")
    );
    assert!(
        build_error("func main() { var d = [\"a\": 1]\n d.updateValue(1) }")
            .contains("updateValue")
    );
    assert!(
        build_error("func main() { let a = [1]\n print(a[0, default: 1]) }").contains("default")
    );
}

#[test]
fn appending_through_a_default_creates_and_extends_the_entry_in_place() {
    assert_eq!(
        main_printing(
            "var groups: [String: [Int]] = [:]\ngroups[\"a\", default: []].append(1)\ngroups[\"b\", default: []].append(2)\ngroups[\"a\", default: []].append(3)\nprint(groups.count)\nprint(groups[\"a\"]?.count ?? 0)\nprint((groups[\"a\"] ?? [])[1])\nprint((groups[\"b\"] ?? [])[0])\nvar words: [Int: [String]] = [:]\nfor w in [\"x\", \"yy\", \"z\", \"ww\", \"v\"] {\n  words[w.count, default: []].append(w)\n}\nprint(words[1]?.joined(separator: \",\") ?? \"\")\nprint(words[2]?.joined(separator: \",\") ?? \"\")"
        ),
        "2\n2\n3\n2\nx,z,v\nyy,ww\n"
    );
}

#[test]
fn changing_through_a_default_does_not_touch_a_copy() {
    assert_eq!(
        main_printing(
            "var a: [String: [Int]] = [\"k\": [1]]\nlet b = a\na[\"k\", default: []].append(2)\na[\"n\", default: []].append(9)\nprint(a[\"k\"]?.count ?? 0)\nprint(b[\"k\"]?.count ?? 0)\nprint(a.count)\nprint(b.count)\nlet row = a[\"k\"] ?? []\na[\"k\", default: []].append(3)\nprint(row.count)"
        ),
        "2\n1\n2\n1\n2\n"
    );
}

#[test]
fn other_updates_work_through_a_default_and_on_members() {
    assert_eq!(
        run(
            "class Board {\n  var rows: [Int: [Int]]\n  init() { self.rows = [:] }\n  func add(_ r: Int, _ v: Int) { rows[r, default: []].append(v) }\n}\nstruct Index {\n  var lists: [String: [String]]\n}\nfunc main() {\n  var d: [String: [Int]] = [:]\n  d[\"a\", default: [0, 0]][1] = 5\n  d[\"a\", default: []].insert(7, at: 0)\n  d[\"a\", default: []].removeLast()\n  print(d[\"a\"]?.count ?? 0)\n  print((d[\"a\"] ?? [])[0])\n  d[\"a\", default: []].removeAll()\n  print(d[\"a\"]?.count ?? -1)\n  var i = Index(lists: [:])\n  i.lists[\"x\", default: []].append(\"p\")\n  i.lists[\"x\", default: []].append(\"q\")\n  print(i.lists[\"x\"]?.count ?? 0)\n  let b = Board()\n  b.add(1, 10)\n  b.add(1, 20)\n  print(b.rows[1]?.count ?? 0)\n  var nested: [Int: [String: [Int]]] = [:]\n  nested[1, default: [:]][\"k\", default: []].append(4)\n  print((nested[1] ?? [:])[\"k\"]?.count ?? 0)\n}"
        ),
        "2\n7\n0\n2\n2\n1\n"
    );
}

#[test]
fn appending_through_a_default_does_not_copy_the_entry_each_time() {
    let source = "func main() {\n  var d: [Int: [Int]] = [:]\n  var i = 0\n  while i < 100000 {\n    d[i % 3, default: []].append(i)\n    i += 1\n  }\n  print(d[0]?.count ?? 0)\n  print(d.count)\n}";
    assert_eq!(run(source), "33334\n3\n");
}

#[test]
fn a_default_update_checks_its_dictionary_and_constants() {
    assert!(
        build_error("func main() { let d: [Int: [Int]] = [:]\n d[1, default: []].append(2) }")
            .contains("constant")
    );
    assert!(
        build_error("func main() { var d: [Int: [Int]] = [:]\n d[1, default: [\"x\"]].append(2) }")
            .contains("expected Int")
    );
    assert!(
        build_error("func main() { var d: [Int: [Int]] = [:]\n d[1, default: []].append(\"x\") }")
            .contains("expected Int")
    );
    assert!(
        build_error("func main() { var a = [[1]]\n a[0, default: []].append(2) }")
            .contains("default")
    );
}

#[test]
fn dictionaries_are_equal_when_they_hold_the_same_entries_in_any_order() {
    assert_eq!(
        main_printing(
            "var a = [\"x\": 1, \"y\": 2]\nvar b: [String: Int] = [:]\nb[\"y\"] = 2\nb[\"x\"] = 1\nprint(a == b)\nprint(a != b)\nb[\"x\"] = 5\nprint(a == b)\nb[\"x\"] = 1\nb[\"z\"] = 0\nprint(a == b)\nb[\"z\"] = nil\nprint(a == b)\nlet e: [String: Int] = [:]\nprint(e == [:])\nprint(a == [:])\na[\"y\"] = nil\nb[\"y\"] = nil\nprint(a == b)\nprint([1: [1, 2]] == [1: [1, 2]])\nprint([1: [1, 2]] == [1: [2, 1]])\nprint([1: \"a\"] == [2: \"a\"])"
        ),
        "true\nfalse\nfalse\nfalse\ntrue\ntrue\nfalse\ntrue\ntrue\nfalse\nfalse\n"
    );
}

#[test]
fn sets_are_equal_when_they_hold_the_same_members_in_any_order() {
    assert_eq!(
        main_printing(
            "var a: Set<Int> = [1, 2, 3]\nvar b: Set<Int> = [3, 2, 1]\nprint(a == b)\nb.remove(2)\nprint(a == b)\nprint(a != b)\nb.insert(2)\nb.insert(2)\nprint(a == b)\nlet e: Set<String> = []\nprint(e == [])\nprint(a == [1, 2, 3, 3])\nlet s: Set<[Int]> = [[1], [2]]\nprint(s == [[2], [1]])"
        ),
        "true\nfalse\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn dictionaries_and_sets_with_struct_keys_and_values_compare_by_value() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var s: String\n}\nfunc main() {\n  let a = [P(x: 1, s: \"a\"): P(x: 2, s: \"b\")]\n  var b: [P: P] = [:]\n  b[P(x: 1, s: \"a\")] = P(x: 2, s: \"b\")\n  print(a == b)\n  b[P(x: 1, s: \"a\")] = P(x: 3, s: \"b\")\n  print(a == b)\n  let s1: Set<P> = [P(x: 1, s: \"a\")]\n  var s2: Set<P> = []\n  s2.insert(P(x: 1, s: \"a\"))\n  print(s1 == s2)\n}"
        ),
        "true\nfalse\ntrue\n"
    );
}

#[test]
fn dictionary_equality_works_in_generic_code_and_nested_collections() {
    assert_eq!(
        run(
            "func same<T: Equatable>(_ a: T, _ b: T) -> Bool { return a == b }\nfunc main() {\n  print(same([1: 2], [1: 2]))\n  print(same([1: 2], [1: 3]))\n  let nested: [[String: Int]] = [[\"a\": 1], [:]]\n  print(nested == [[\"a\": 1], [:]])\n  print(nested == [[:], [\"a\": 1]])\n  print(same([1, 2], [1, 2]))\n}"
        ),
        "true\nfalse\ntrue\nfalse\ntrue\n"
    );
}

#[test]
fn comparing_collections_without_equatable_values_is_rejected() {
    assert!(
        build_error("class C {}\nfunc main() {\n  let a: [Int: C] = [:]\n  print(a == a)\n}")
            .contains("equality")
    );
    assert!(build_error("func main() { print([1: 2] == [\"a\": 2]) }").contains("expected"));
    assert!(
        build_error("func main() { let s: Set<Int> = [1]\n let d = [1: 2]\n print(s == d) }")
            .contains("expected")
    );
}

#[test]
fn union_intersection_subtracting_and_symmetric_difference_build_new_sets() {
    let output = main_printing(
        "let a: Set<Int> = [1, 2, 3]\nlet b: Set<Int> = [3, 4]\nlet u = a.union(b)\nlet i = a.intersection(b)\nlet s = a.subtracting(b)\nlet x = a.symmetricDifference(b)\nprint(u.count)\nprint(i.count)\nprint(s.count)\nprint(x.count)\nvar order = 0\nfor m in u { order = order * 10 + m }\nprint(order)\nprint(i.contains(3))\nprint(s.contains(1))\nprint(x.contains(4))\nprint(x.contains(3))\nprint(a.count)\nprint(b.count)\nprint(a.union([9]).count)",
    );
    assert_eq!(
        output,
        "4\n1\n2\n3\n1234\ntrue\ntrue\ntrue\nfalse\n3\n2\n4\n"
    );
}

#[test]
fn subset_superset_and_disjoint_compare_members() {
    assert_eq!(
        main_printing(
            "let a: Set<Int> = [1, 2]\nlet b: Set<Int> = [1, 2, 3]\nlet c: Set<Int> = [7]\nlet e: Set<Int> = []\nprint(a.isSubset(of: b))\nprint(b.isSubset(of: a))\nprint(b.isSuperset(of: a))\nprint(a.isSuperset(of: b))\nprint(a.isSubset(of: a))\nprint(e.isSubset(of: a))\nprint(a.isDisjoint(with: c))\nprint(a.isDisjoint(with: b))\nprint(e.isDisjoint(with: e))\nprint(a.isSubset(of: [1, 2, 5]))"
        ),
        "true\nfalse\ntrue\nfalse\ntrue\ntrue\ntrue\nfalse\ntrue\ntrue\n"
    );
}

#[test]
fn set_operations_work_for_strings_structs_and_in_generic_code() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nfunc both<T>(_ a: Set<T>, _ b: Set<T>) -> Int { return a.intersection(b).count }\nfunc main() {\n  let s: Set<String> = [\"a\", \"b\"]\n  let t: Set<String> = [\"b\", \"c\"]\n  print(s.union(t).count)\n  print(s.subtracting(t).contains(\"a\"))\n  let p: Set<P> = [P(x: 1), P(x: 2)]\n  let q: Set<P> = [P(x: 2)]\n  print(p.intersection(q).contains(P(x: 2)))\n  print(q.isSubset(of: p))\n  let s1: Set<Int> = [1, 2, 3]\n  let s2: Set<Int> = [2, 3]\n  print(both(s1, s2))\n}"
        ),
        "3\ntrue\ntrue\ntrue\n2\n"
    );
}

#[test]
fn set_operations_check_their_arguments() {
    let head = "func main() {\n  let a: Set<Int> = [1]\n  let d = [1: 2]\n";
    for (call, message) in [
        ("a.union()", "union"),
        ("a.union(of: a)", "union"),
        ("a.isSubset(a)", "isSubset"),
        ("a.isSuperset(with: a)", "isSuperset"),
        ("a.isDisjoint(of: a)", "isDisjoint"),
        ("a.union([\"x\"])", "expected"),
        ("a.intersection(d)", "expected"),
        ("a.subtracting(1)", "expected"),
    ] {
        let source = format!("{head}  print({call})\n}}");
        assert!(build_error(&source).contains(message), "{call}");
    }
    assert!(
        build_error("func main() { let d = [1: 2]\n print(d.union(d)) }").contains("no method")
    );
}

#[test]
fn empty_collections_are_made_with_their_type_names() {
    assert_eq!(
        main_printing(
            "var a = [Int]()\na.append(1)\nvar d = [String: Int]()\nd[\"k\"] = 2\nvar s = Set<Int>()\ns.insert(3)\nvar n = Array<String>()\nn.append(\"x\")\nvar m = Dictionary<Int, Int>()\nm[1] = 1\nvar g = [[Int]]()\ng.append([1])\nvar t: Set<String> = Set()\nt.insert(\"q\")\nvar e: [Int: [Int]] = Dictionary()\ne[1, default: []].append(4)\nprint(a.count + d.count + s.count + n.count + m.count + g.count + t.count + e.count)\nprint(d[\"k\"] ?? 0)\nprint(e[1]?.count ?? 0)"
        ),
        "8\n2\n1\n"
    );
}

#[test]
fn a_set_is_built_from_an_array_a_literal_or_another_set() {
    assert_eq!(
        main_printing(
            "let xs = [3, 1, 3, 2, 1]\nlet s = Set(xs)\nprint(s.count)\nprint(s.contains(2))\nlet t = Set([\"a\", \"b\", \"a\"])\nprint(t.count)\nlet u = Set<Int>([1, 1, 2])\nprint(u.count)\nlet v: Set<Int> = Set(xs)\nprint(v == s)\nlet w = Set(s)\nprint(w == s)\nvar order = 0\nfor m in s { order = order * 10 + m }\nprint(order)\nprint(Set([[1], [1], [2]]).count)\nprint(Set(\"hello world\".split(separator: \" \")).count)"
        ),
        "3\ntrue\n2\n2\ntrue\ntrue\n312\n2\n2\n"
    );
}

#[test]
fn an_array_is_built_by_repeating_a_value_or_from_a_set_or_array() {
    assert_eq!(
        main_printing(
            "var zeros = [Int](repeating: 0, count: 4)\nzeros[2] = 7\nprint(zeros.count)\nprint(zeros[2] + zeros[0])\nlet names = Array(repeating: \"x\", count: 3)\nprint(names.joined())\nlet none = [Int](repeating: 1, count: 0)\nprint(none.isEmpty)\nlet grid = [[Int]](repeating: [0, 0], count: 2)\nprint(grid.count)\nlet s: Set<Int> = [5, 6]\nlet fromSet = Array(s)\nprint(fromSet.count)\nprint(fromSet[0] + fromSet[1])\nlet copy = Array(fromSet)\nprint(copy == fromSet)\nlet sized: [Int?] = []\nprint(sized.count)"
        ),
        "4\n7\nxxx\ntrue\n2\n2\n11\ntrue\n0\n"
    );
}

#[test]
fn a_repeated_array_does_not_share_its_elements() {
    assert_eq!(
        main_printing(
            "var grid = [[Int]](repeating: [0, 0], count: 2)\ngrid[0][1] = 5\nprint(grid[0][1])\nprint(grid[1][1])\nvar objects = [String](repeating: \"a\", count: 2)\nobjects[0] += \"b\"\nprint(objects[0])\nprint(objects[1])"
        ),
        "5\n0\nab\na\n"
    );
}

#[test]
fn a_negative_repeat_count_traps() {
    run_expecting_a_crash(
        "func main() {\n  var n = 0\n  n -= 1\n  let a = [Int](repeating: 0, count: n)\n  print(a.count)\n}",
    );
}

#[test]
fn collection_initializers_check_their_arguments() {
    assert!(build_error("func main() { let a = [Int](1) }").contains("Array cannot be built"));
    assert!(build_error("func main() { let a = Array() }").contains("element type"));
    assert!(build_error("func main() { let d = Dictionary() }").contains("key and value"));
    assert!(build_error("func main() { let s = Set() }").contains("element type"));
    assert!(build_error("func main() { let d = [Int: Int](1) }").contains("no arguments"));
    assert!(build_error("func main() { let s = Set(1) }").contains("Set cannot be built"));
    assert!(build_error("func main() { let s = Set([1], [2]) }").contains("Set takes"));
    assert!(build_error("func main() { let a = Array(5) }").contains("Array cannot be built"));
    assert!(
        build_error("func main() { let a = [Int](repeating: \"x\", count: 1) }")
            .contains("expected Int")
    );
    assert!(
        build_error("func main() { let a = [Int](repeating: 0, size: 1) }").contains("Array takes")
    );
    assert!(
        build_error("class C {}\nfunc main() { let s = Set<C>() }").contains("cannot be a key")
    );
    assert!(build_error("func main() { let a = [1, 2]()\n }").contains("collection type"));
}

#[test]
fn a_closure_is_passed_to_a_function_and_called() {
    assert_eq!(
        run(
            "func apply(_ f: (Int) -> Int, _ x: Int) -> Int {\n  return f(x)\n}\nfunc twice(_ f: (Int) -> Int, _ x: Int) -> Int {\n  return f(f(x))\n}\nfunc main() {\n  print(apply({ x in x + 1 }, 4))\n  print(apply({ $0 * 3 }, 4))\n  print(twice({ $0 + 10 }, 1))\n  print(apply({ (a: Int) -> Int in return a - 1 }, 4))\n}"
        ),
        "5\n12\n21\n3\n"
    );
}

#[test]
fn a_trailing_closure_is_the_last_argument() {
    assert_eq!(
        run(
            "func repeatTwice(_ n: Int, _ body: (Int) -> Void) {\n  body(n)\n  body(n + 1)\n}\nfunc main() {\n  repeatTwice(5) { i in\n    print(i)\n  }\n}"
        ),
        "5\n6\n"
    );
}

#[test]
fn a_closure_reads_and_changes_the_variables_it_captures() {
    assert_eq!(
        run(
            "func each(_ n: Int, _ body: (Int) -> Void) {\n  var i = 0\n  while i < n {\n    body(i)\n    i += 1\n  }\n}\nfunc main() {\n  var total = 0\n  let bonus = 100\n  each(4) { i in\n    total += i + bonus\n  }\n  print(total)\n  var log = \"\"\n  each(3) { i in log += String(i) }\n  print(log)\n  total = 0\n  each(2) { _i in total += 1 }\n  print(total)\n}"
        ),
        "406\n012\n2\n"
    );
}

#[test]
fn a_function_declared_inside_a_function_captures_and_recurses() {
    assert_eq!(
        run(
            "func main() {\n  var calls = 0\n  let base = 10\n  func add(_ x: Int) -> Int {\n    calls += 1\n    return x + base\n  }\n  func fact(_ n: Int) -> Int {\n    if n <= 1 { return 1 }\n    return n * fact(n - 1)\n  }\n  func greet(name: String) -> String {\n    return \"hi \" + name\n  }\n  print(add(1) + add(2))\n  print(calls)\n  print(fact(5))\n  print(greet(name: \"x\"))\n}"
        ),
        "23\n2\n120\nhi x\n"
    );
}

#[test]
fn a_local_function_can_call_an_earlier_one_and_be_passed_on() {
    assert_eq!(
        run(
            "func apply(_ f: (Int) -> Int, _ x: Int) -> Int { return f(x) }\nfunc main() {\n  func inc(_ x: Int) -> Int { return x + 1 }\n  func inc2(_ x: Int) -> Int { return inc(inc(x)) }\n  print(apply(inc2, 5))\n  let triple = { (x: Int) -> Int in x * 3 }\n  print(apply(triple, 4))\n  print(triple(5))\n  let shout: (String) -> String = { $0 + \"!\" }\n  print(shout(\"hey\"))\n}"
        ),
        "7\n12\n15\nhey!\n"
    );
}

#[test]
fn closures_nest_and_capture_through_each_other() {
    assert_eq!(
        run(
            "func apply(_ f: (Int) -> Int, _ x: Int) -> Int { return f(x) }\nfunc main() {\n  var total = 0\n  let k = 7\n  let r = apply({ a in\n    apply({ b in\n      total += a + b + k\n      return total\n    }, 2)\n  }, 1)\n  print(r)\n  print(total)\n}"
        ),
        "10\n10\n"
    );
}

#[test]
fn a_closure_captures_loop_variables_strings_arrays_and_instances() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(\"free\") }\n}\nfunc run(_ body: () -> Void) { body() }\nfunc main() {\n  var words: [String] = []\n  for i in 0..<3 {\n    let label = \"w\" + String(i)\n    run { words.append(label) }\n  }\n  print(words.joined(separator: \",\"))\n  let box = Box(n: 1)\n  run { box.n += 5 }\n  print(box.n)\n  var s = \"a\"\n  run { s += \"b\" }\n  run { s += \"c\" }\n  print(s)\n}"
        ),
        "w0,w1,w2\n6\nabc\nfree\n"
    );
}

#[test]
fn a_closure_result_type_is_inferred_for_a_generic_function() {
    assert_eq!(
        run(
            "func transform<T, U>(_ x: T, _ f: (T) -> U) -> U { return f(x) }\nfunc main() {\n  print(transform(3) { $0 + 1 })\n  print(transform(\"a\") { $0 + \"b\" })\n  print(transform(5) { String($0) + \"!\" })\n  print(transform(2, { x in x * x }))\n  let n: Int = transform(\"abc\") { $0.count }\n  print(n)\n}"
        ),
        "4\nab\n5!\n4\n3\n"
    );
}

#[test]
fn a_generic_function_may_build_and_call_its_own_closures() {
    assert_eq!(
        run(
            "func wrap<T>(_ x: T, _ f: (T) -> T) -> T {\n  let g = { (a: T) -> T in f(a) }\n  return g(x)\n}\nfunc both<T>(_ x: T, _ f: (T) -> T) -> T {\n  func again(_ v: T) -> T { return f(f(v)) }\n  return again(x)\n}\nfunc main() {\n  print(wrap(2) { $0 + 5 })\n  print(wrap(\"a\") { $0 + \"!\" })\n  print(both(1) { $0 * 10 })\n  print(both(\"x\") { $0 + $0 })\n}"
        ),
        "7\na!\n100\nxxxx\n"
    );
}

#[test]
fn methods_and_initializers_take_closures() {
    assert_eq!(
        run(
            "struct Counter {\n  var n: Int\n  func apply(_ f: (Int) -> Int) -> Int { return f(n) }\n  func each(_ body: (Int) -> Void) {\n    var i = 0\n    while i < n {\n      body(i)\n      i += 1\n    }\n  }\n}\nclass Box<T> {\n  var value: T\n  func with<U>(_ f: (T) -> U) -> U { return f(value) }\n}\nfunc main() {\n  let c = Counter(n: 3)\n  print(c.apply { $0 * 7 })\n  var s = 0\n  c.each { s += $0 }\n  print(s)\n  let b = Box(value: 4)\n  print(b.with { $0 + 1 })\n  print(b.with { String($0) })\n}"
        ),
        "21\n3\n5\n4\n"
    );
}

#[test]
fn a_method_closure_uses_self_and_its_members() {
    assert_eq!(
        run(
            "func run(_ body: () -> Void) { body() }\nclass Counter {\n  var n: Int\n  func bump(by k: Int) {\n    run { n += k }\n  }\n  func twice() {\n    run { bump(by: 1) }\n    run { self.bump(by: 2) }\n  }\n}\nfunc main() {\n  let c = Counter(n: 0)\n  c.twice()\n  print(c.n)\n}"
        ),
        "3\n"
    );
}

#[test]
fn closure_misuse_is_rejected_with_a_message() {
    let head = "func apply(_ f: (Int) -> Int, _ x: Int) -> Int { return f(x) }\n";
    for (body, message) in [
        (
            "print(apply({ a, b in a }, 1))",
            "takes 2 parameter(s), but 1 are expected",
        ),
        ("print(apply({ $1 }, 1))", "cannot find '$1' in scope"),
        ("print(apply({ \"x\" }, 1))", "expected Int"),
        ("print(apply({ x in x + true }, 1))", "expected Int"),
        (
            "let f = { x in x }\n  print(f(1))",
            "cannot infer the type of 'x'",
        ),
        (
            "let n = 1\n  print(apply({ x in n = 2\n return x }, 1))",
            "constant",
        ),
        ("print(apply(5, 1))", "expected"),
        ("print(apply({ (x: String) -> Int in 1 }, 1))", "expected"),
        (
            "let g = { (x: Int) -> Int in x }\n  print(g(1, 2))",
            "takes 1 argument(s), but 2 were given",
        ),
        (
            "let g = { (x: Int) -> Int in x }\n  print(g(\"s\"))",
            "expected Int",
        ),
        (
            "print(apply({ (x: Int) -> Int in\n if x > 0 { return 1 }\n }, 1))",
            "must return a value on every path",
        ),
    ] {
        let source = format!("{head}func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_local_function_checks_its_signature_and_labels() {
    assert!(
        build_error("func main() {\n  func f(a: Int) -> Int { return a }\n  print(f(1))\n}")
            .contains("label")
    );
    assert!(
        build_error(
            "func main() {\n  func f(_ a: Int) -> Int {\n    if a > 0 { return 1 }\n  }\n}"
        )
        .contains("must return a value on every path")
    );
}

#[test]
fn map_filter_and_reduce_build_arrays_and_values_from_closures() {
    assert_eq!(
        main_printing(
            "let xs = [1, 2, 3, 4, 5]\nlet doubled = xs.map { $0 * 2 }\nlet evens = xs.filter { $0 % 2 == 0 }\nlet total = xs.reduce(0) { $0 + $1 }\nlet words = xs.map { String($0) }\nprint(doubled.count)\nprint(doubled[4])\nprint(evens.count)\nprint(evens[1])\nprint(total)\nprint(words.joined(separator: \"-\"))\nlet joined = xs.reduce(\"\") { acc, x in acc + String(x) }\nprint(joined)\nprint(xs.map { $0 > 2 }.filter { $0 }.count)\nlet nested = [[1, 2], [3]].map { $0.count }\nprint(nested[0] + nested[1])"
        ),
        "5\n10\n2\n4\n15\n1-2-3-4-5\n12345\n3\n3\n"
    );
}

#[test]
fn for_each_contains_where_and_first_where_search_with_closures() {
    assert_eq!(
        main_printing(
            "let xs = [3, 8, 5, 12]\nvar total = 0\nxs.forEach { total += $0 }\nprint(total)\nprint(xs.contains { $0 > 10 })\nprint(xs.contains(where: { $0 > 20 }))\nprint(xs.first { $0 > 4 } ?? -1)\nprint(xs.first(where: { $0 > 99 }) == nil)\nprint(xs.firstIndex { $0 == 5 } ?? -1)\nprint(xs.allSatisfy { $0 > 2 })\nprint(xs.allSatisfy { $0 > 3 })\nlet e: [Int] = []\nprint(e.allSatisfy { $0 > 100 })\nvar seen: [String] = []\n[\"a\", \"b\"].forEach { seen.append($0 + \"!\") }\nprint(seen.joined())"
        ),
        "28\ntrue\nfalse\n8\ntrue\n2\ntrue\nfalse\ntrue\na!b!\n"
    );
}

#[test]
fn sorted_orders_comparable_elements_and_takes_a_comparison_closure() {
    assert_eq!(
        main_printing(
            "let xs = [5, 2, 9, 1, 5, 6]\nlet up = xs.sorted()\nlet down = xs.sorted { $0 > $1 }\nvar a = 0\nfor x in up { a = a * 10 + x }\nvar b = 0\nfor x in down { b = b * 10 + x }\nprint(a)\nprint(b)\nprint(xs.count)\nlet names = [\"pear\", \"apple\", \"fig\"]\nprint(names.sorted().joined(separator: \",\"))\nprint(names.sorted(by: { $0.count < $1.count }).joined(separator: \",\"))\nlet e: [Int] = []\nprint(e.sorted().count)\nprint([7].sorted()[0])\nlet chars: [Character] = [\"c\", \"a\", \"b\"]\nprint(String(chars.sorted()[0]))"
        ),
        "125569\n965521\n6\napple,fig,pear\nfig,pear,apple\n0\n7\na\n"
    );
}

#[test]
fn sorting_is_stable_and_works_on_large_arrays_and_structs() {
    assert_eq!(
        run(
            "struct P {\n  var key: Int\n  var tag: Int\n}\nfunc main() {\n  let ps = [P(key: 2, tag: 0), P(key: 1, tag: 1), P(key: 2, tag: 2), P(key: 1, tag: 3)]\n  let sorted = ps.sorted { $0.key < $1.key }\n  var order = 0\n  for p in sorted { order = order * 10 + p.tag }\n  print(order)\n  var big: [Int] = []\n  var i = 0\n  while i < 5000 {\n    big.append((i * 7919) % 5003)\n    i += 1\n  }\n  let s = big.sorted()\n  var ok = true\n  var j = 1\n  while j < s.count {\n    if s[j - 1] > s[j] { ok = false }\n    j += 1\n  }\n  print(ok)\n  print(s.count)\n}"
        ),
        "1302\ntrue\n5000\n"
    );
}

#[test]
fn closures_that_map_classes_and_strings_release_everything() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(n) }\n}\nfunc main() {\n  let boxes = [Box(n: 1), Box(n: 2)]\n  let ids = boxes.map { $0.n * 10 }\n  print(ids[1])\n  let kept = boxes.filter { $0.n > 1 }\n  print(kept.count)\n  print(boxes.contains { $0.n == 2 })\n}"
        ),
        "20\n1\ntrue\n1\n2\n"
    );
}

#[test]
fn a_function_value_can_be_given_to_a_higher_order_member() {
    assert_eq!(
        run(
            "func double(_ x: Int) -> Int { return x * 2 }\nfunc main() {\n  func triple(_ x: Int) -> Int { return x * 3 }\n  let f = { (x: Int) -> Int in x + 1 }\n  print([1, 2].map(triple)[1])\n  print([1, 2].map(f)[0])\n  print([4].reduce(0, { $0 + $1 }))\n}"
        ),
        "6\n2\n4\n"
    );
}

#[test]
fn higher_order_members_check_their_closures() {
    let head = "func main() {\n  let xs = [1, 2]\n  ";
    for (body, message) in [
        (
            "print(xs.map { }.count)",
            "map needs a closure that returns a value",
        ),
        ("print(xs.filter { $0 }.count)", "expected Bool"),
        ("print(xs.map { $0 + \"a\" }.count)", "expected Int"),
        ("print(xs.reduce(0) { $0 + $1 + true })", "expected Int"),
        ("print(xs.reduce(0, 5))", "expected"),
        (
            "print([[1]].sorted().count)",
            "needs elements that can be ordered",
        ),
        (
            "print(xs.map({ a, b in a }).count)",
            "takes 2 parameter(s), but 1 are expected",
        ),
        ("print(xs.sorted(by: 5).count)", "expected"),
        (
            "print(xs.filter(where: { $0 > 1 }).count)",
            "takes a closure",
        ),
    ] {
        let source = format!("{head}{body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_set_offers_the_higher_order_members_of_an_array() {
    assert_eq!(
        main_printing(
            "let s: Set<Int> = [3, 1, 2, 5]\nlet doubled = s.map { $0 * 2 }\nlet big = s.filter { $0 > 1 }\nprint(doubled.count)\nprint(doubled[0])\nprint(big.count)\nprint(big.contains(5))\nprint(big.contains(1))\nprint(s.reduce(0) { $0 + $1 })\nvar sum = 0\ns.forEach { sum += $0 }\nprint(sum)\nprint(s.contains { $0 > 4 })\nprint(s.allSatisfy { $0 > 0 })\nprint(s.first { $0 > 2 } ?? 0)\nlet sorted = s.sorted()\nprint(sorted[0] * 10 + sorted[3])\nprint(s.sorted { $0 > $1 }[0])\nprint(s.contains(3))"
        ),
        "4\n6\n3\ntrue\nfalse\n11\n11\ntrue\ntrue\n3\n15\n5\ntrue\n"
    );
}

#[test]
fn a_dictionary_maps_values_filters_and_walks_its_entries_with_closures() {
    assert_eq!(
        main_printing(
            "let d = [\"a\": 1, \"b\": 2, \"c\": 3]\nlet doubled = d.mapValues { $0 * 2 }\nlet labels = d.mapValues { \"#\" + String($0) }\nlet odd = d.filter { k, v in v % 2 == 1 }\nprint(doubled[\"c\"] ?? 0)\nprint(labels[\"b\"] ?? \"\")\nprint(odd.count)\nprint(odd[\"b\"] == nil)\nvar total = 0\nvar keys = \"\"\nd.forEach { k, v in\n  total += v\n  keys += k\n}\nprint(total)\nprint(keys)\nprint(d.contains { $0 == \"b\" && $1 == 2 })\nprint(d.allSatisfy { $1 > 0 })\nprint(d.keys.sorted().joined())\nprint(d.values.reduce(0) { $0 + $1 })\nprint(doubled == [\"a\": 2, \"b\": 4, \"c\": 6])"
        ),
        "6\n#2\n2\ntrue\n6\nabc\ntrue\ntrue\nabc\n6\ntrue\n"
    );
}

#[test]
fn a_dictionary_groups_an_array_by_a_closure() {
    assert_eq!(
        main_printing(
            "let words = [\"apple\", \"fig\", \"kiwi\", \"pear\", \"plum\", \"date\"]\nlet byLength = Dictionary(grouping: words, by: { $0.count })\nprint(byLength.count)\nprint(byLength[3]?.count ?? 0)\nprint(byLength[4]?.joined(separator: \",\") ?? \"\")\nprint((byLength[5] ?? [])[0])\nlet parity = Dictionary(grouping: [1, 2, 3, 4, 5], by: { $0 % 2 == 0 })\nprint(parity[true]?.count ?? 0)\nprint(parity[false]?.count ?? 0)\nlet empty = Dictionary(grouping: [Int](), by: { $0 })\nprint(empty.isEmpty)"
        ),
        "3\n1\nkiwi,pear,plum,date\napple\n2\n3\ntrue\n"
    );
}

#[test]
fn the_collection_closures_report_a_wrong_use() {
    for (body, message) in [
        (
            "let d = [1: 2]\n  print(d.mapValues { }.count)",
            "mapValues needs a closure that returns a value",
        ),
        (
            "let d = [1: 2]\n  print(d.filter { $0 }.count)",
            "expected Bool",
        ),
        (
            "let d = [1: 2]\n  print(d.filter { a in true }.count)",
            "takes 1 parameter(s), but 2 are expected",
        ),
        (
            "let s: Set<Int> = [1]\n  print(s.map { }.count)",
            "map needs a closure that returns a value",
        ),
        (
            "let s: Set<Int> = [1]\n  print(s.sorted(by: 3).count)",
            "expected",
        ),
        (
            "let d = Dictionary(grouping: 5, by: { $0 })",
            "cannot group",
        ),
        (
            "let d = Dictionary(grouping: [1], by: { $0 }, 3)",
            "Dictionary takes",
        ),
        (
            "let d = [1: 2]\n  print(d.map { $0 }.count)",
            "no method 'map'",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn compact_map_keeps_the_present_values_and_flat_map_joins_arrays() {
    assert_eq!(
        main_printing(
            "let xs = [1, 2, 3, 4]\nlet halves = xs.compactMap { (x: Int) -> Int? in\n  if x % 2 == 0 { return nil }\n  return x * 10\n}\nprint(halves.count)\nprint(halves[1])\nlet words = [\"a\", \"\", \"bc\"]\nlet firsts = words.compactMap { $0.first }\nprint(firsts.count)\nlet flat = [[1, 2], [], [3]].flatMap { $0 }\nprint(flat.count)\nprint(flat[2])\nlet pairs = xs.flatMap { [$0, $0 * 100] }\nprint(pairs.count)\nprint(pairs[7])\nlet letters = [[1, 2], [3]].flatMap { row in row.map { v in v * 2 } }\nprint(letters.count)"
        ),
        "2\n30\n2\n3\n3\n8\n400\n3\n"
    );
}

#[test]
fn min_max_and_count_search_with_orderings_and_predicates() {
    assert_eq!(
        main_printing(
            "let xs = [4, 9, 2, 7]\nprint(xs.min() ?? -1)\nprint(xs.max() ?? -1)\nprint(xs.min(by: { $0 > $1 }) ?? -1)\nprint(xs.max { $0 > $1 } ?? -1)\nlet e: [Int] = []\nprint(e.min() == nil)\nprint(e.max(by: { $0 < $1 }) == nil)\nprint(xs.count(where: { $0 > 3 }))\nprint(xs.count { $0 > 100 })\nprint([\"pear\", \"fig\", \"apple\"].min() ?? \"\")\nprint([\"pear\", \"fig\", \"apple\"].max(by: { $0.count < $1.count }) ?? \"\")"
        ),
        "2\n9\n9\n2\ntrue\ntrue\n3\n0\napple\napple\n"
    );
}

#[test]
fn sort_orders_an_array_in_place_and_leaves_a_copy_alone() {
    assert_eq!(
        main_printing(
            "var xs = [3, 1, 2]\nlet before = xs\nxs.sort()\nprint(xs[0] * 100 + xs[1] * 10 + xs[2])\nprint(before[0])\nxs.sort { $0 > $1 }\nprint(xs[0])\nxs.sort(by: { $0 < $1 })\nprint(xs[0])\nvar names = [\"b\", \"c\", \"a\"]\nnames.sort()\nprint(names.joined())"
        ),
        "123\n3\n3\n1\nabc\n"
    );
}

#[test]
fn reduce_into_changes_its_accumulator_in_the_closure() {
    assert_eq!(
        main_printing(
            "let xs = [1, 2, 3, 4]\nlet evens = xs.reduce(into: [Int]()) { acc, x in\n  if x % 2 == 0 { acc.append(x) }\n}\nprint(evens.count)\nprint(evens[1])\nlet counts = [\"a\", \"b\", \"a\"].reduce(into: [String: Int]()) { acc, w in\n  acc[w, default: 0] += 1\n}\nprint(counts[\"a\"] ?? 0)\nlet total = xs.reduce(into: 0) { $0 += $1 }\nprint(total)\nlet text = xs.reduce(into: \"\") { s, x in s += String(x) }\nprint(text)"
        ),
        "2\n4\n2\n10\n1234\n"
    );
}

#[test]
fn a_set_offers_the_new_closure_members() {
    assert_eq!(
        main_printing(
            "let s: Set<Int> = [5, 2, 8]\nprint(s.min() ?? 0)\nprint(s.max { $0 < $1 } ?? 0)\nprint(s.count(where: { $0 > 3 }))\nprint(s.compactMap { (x: Int) -> Int? in\n  if x > 2 { return x }\n  return nil\n}.count)\nprint(s.flatMap { [$0, $0] }.count)\nlet grouped = s.reduce(into: [Int]()) { acc, x in acc.append(x) }\nprint(grouped.count)"
        ),
        "2\n8\n2\n2\n6\n3\n"
    );
}

#[test]
fn the_new_closure_members_check_their_arguments() {
    for (body, message) in [
        (
            "let xs = [1]\n  print(xs.compactMap { $0 }.count)",
            "compactMap needs a closure that returns an optional",
        ),
        (
            "let xs = [1]\n  print(xs.flatMap { $0 }.count)",
            "flatMap needs a closure that returns an array",
        ),
        (
            "let xs = [[1]]\n  print(xs.min())",
            "needs elements that can be ordered",
        ),
        (
            "var xs = [[1]]\n  xs.sort()",
            "needs elements that can be ordered",
        ),
        (
            "let xs = [1]\n  print(xs.reduce(into: 0, 5))",
            "reduce(into:) takes a closure",
        ),
        (
            "let xs = [1]\n  print(xs.reduce(into: 0) { a in a })",
            "takes a closure with two parameters",
        ),
        (
            "let xs = [1]\n  print(xs.count(of: 1))",
            "no method 'count'",
        ),
        ("let xs = [1]\n  print(xs.max(by: 5))", "expected"),
        ("let xs = [1]\n  let c = xs.sort()", "no method 'sort'"),
        ("var xs = [1]\n  xs.sort(1, 2)", "sort takes"),
        ("let xs = [1]\n  xs.sort()", "constant"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn an_operator_can_be_given_where_a_function_is_expected() {
    assert_eq!(
        run(
            "func fold(_ xs: [Int], _ start: Int, _ f: (Int, Int) -> Int) -> Int {\n  var acc = start\n  for x in xs { acc = f(acc, x) }\n  return acc\n}\nfunc test(_ a: Int, _ b: Int, _ p: (Int, Int) -> Bool) -> Bool { return p(a, b) }\nfunc main() {\n  let xs = [3, 1, 4, 1, 5]\n  print(xs.reduce(0, +))\n  print(xs.reduce(1, *))\n  print(fold(xs, 100, -))\n  print(xs.sorted(by: <)[0])\n  print(xs.sorted(by: >)[0])\n  print(xs.min(by: <) ?? -1)\n  print(xs.max(by: >) ?? -1)\n  print([\"a\", \"b\"].reduce(\"\", +))\n  print(test(2, 2, ==))\n  print(test(2, 3, !=))\n  print(test(1, 2, <=))\n  print(test(3, 2, >=))\n  print(fold([7, 2], 0, %))\n  print(fold([8, 2], 16, /))\n  print([true, false].reduce(true, &&))\n  print([true, false].reduce(false, ||))\n}"
        ),
        "14\n60\n86\n1\n5\n1\n1\nab\ntrue\ntrue\ntrue\ntrue\n0\n1\nfalse\ntrue\n"
    );
}

#[test]
fn an_operator_function_checks_the_types_it_is_given() {
    for (body, message) in [
        ("print([1].reduce(0, &&))", "expected Bool"),
        ("print([\"a\"].reduce(0, +))", "expected"),
        ("print([1].map(+))", "cannot find '$1'"),
        ("let f = +\n  print(f)", "expected an expression"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_function_can_return_a_closure_that_keeps_its_variables() {
    assert_eq!(
        run(
            "func makeCounter() -> () -> Int {\n  var n = 0\n  return {\n    n += 1\n    return n\n  }\n}\nfunc adder(_ k: Int) -> (Int) -> Int {\n  return { $0 + k }\n}\nfunc main() {\n  let a = makeCounter()\n  let b = makeCounter()\n  print(a())\n  print(a())\n  print(b())\n  let add5 = adder(5)\n  print(add5(1))\n  print(add5(10))\n}"
        ),
        "1\n2\n1\n6\n15\n"
    );
}

#[test]
fn closures_compose_and_a_generic_closure_keeps_its_value() {
    assert_eq!(
        run(
            "func compose(_ f: (Int) -> Int, _ g: (Int) -> Int) -> (Int) -> Int {\n  return { g(f($0)) }\n}\nfunc constant<T>(_ x: T) -> () -> T {\n  return { x }\n}\nfunc main() {\n  let plus = { (x: Int) -> Int in x + 1 }\n  let times = { (x: Int) -> Int in x * 10 }\n  let both = compose(plus, times)\n  print(both(4))\n  let five = constant(5)\n  let word = constant(\"hi\")\n  print(five() + 1)\n  print(word())\n}"
        ),
        "50\n6\nhi\n"
    );
}

#[test]
fn closures_are_stored_in_fields_arrays_optionals_and_dictionaries() {
    assert_eq!(
        run(
            "struct Button {\n  var title: String\n  var action: (Int) -> Int\n}\nclass Holder {\n  var callback: (() -> Void)?\n  init() { self.callback = nil }\n}\nfunc main() {\n  let b = Button(title: \"go\", action: { $0 * 3 })\n  print(b.action(4))\n  var steps: [(Int) -> Int] = []\n  steps.append({ $0 + 1 })\n  steps.append({ $0 * 2 })\n  var value = 5\n  for step in steps { value = step(value) }\n  print(value)\n  var handlers: [String: (Int) -> Int] = [:]\n  handlers[\"double\"] = { $0 * 2 }\n  if let h = handlers[\"double\"] { print(h(21)) }\n  let holder = Holder()\n  var ran = 0\n  holder.callback = { ran += 1 }\n  if let c = holder.callback { c() }\n  if let c = holder.callback { c() }\n  print(ran)\n  var maybe: ((Int) -> Int)? = nil\n  print(maybe == nil)\n}"
        ),
        "12\n12\n42\n2\ntrue\n"
    );
}

#[test]
fn closures_that_share_a_variable_see_each_others_changes_after_escaping() {
    assert_eq!(
        run(
            "struct Counter {\n  var next: () -> Int\n  var reset: () -> Void\n}\nfunc makeCounter() -> Counter {\n  var n = 0\n  return Counter(next: { n += 1\n return n }, reset: { n = 0 })\n}\nfunc main() {\n  let c = makeCounter()\n  print(c.next())\n  print(c.next())\n  c.reset()\n  print(c.next())\n  let d = makeCounter()\n  print(d.next())\n}"
        ),
        "1\n2\n1\n1\n"
    );
}

#[test]
fn a_function_value_can_be_called_where_it_is_produced() {
    assert_eq!(
        run(
            "func adder(_ k: Int) -> (Int) -> Int { return { $0 + k } }\nfunc main() {\n  print(adder(1)(2))\n  let steps: [(Int) -> Int] = [adder(10), adder(20)]\n  print(steps[1](1))\n  var total = 0\n  for s in steps { total += s(0) }\n  print(total)\n}"
        ),
        "3\n21\n30\n"
    );
}

#[test]
fn an_escaped_closure_keeps_what_it_captured_alive_until_it_is_dropped() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(\"free\") }\n}\nfunc make() -> () -> Int {\n  let b = Box(n: 7)\n  return { b.n }\n}\nfunc main() {\n  var f: (() -> Int)? = make()\n  print(\"made\")\n  if let g = f { print(g()) }\n  f = nil\n  print(\"dropped\")\n}"
        ),
        "made\n7\nfree\ndropped\n"
    );
}

#[test]
fn creating_and_dropping_many_closures_keeps_their_results() {
    let source = "func adder(_ k: Int) -> (Int) -> Int {\n  var step = k\n  step += 1\n  return { $0 + step }\n}\nfunc main() {\n  var total = 0\n  var i = 0\n  while i < 100000 {\n    let f = adder(i)\n    total += f(1)\n    let g = [\"a\", \"bb\"].map { $0 + String(i) }\n    total += g.count\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "5000350000\n");
}

#[test]
fn a_closure_that_returns_a_closure_works_and_a_local_function_can_escape() {
    assert_eq!(
        run(
            "func curry(_ a: Int) -> (Int) -> (Int) -> Int {\n  return { b in { c in a * 100 + b * 10 + c } }\n}\nfunc maker() -> (Int) -> Int {\n  var calls = 0\n  func twice(_ x: Int) -> Int {\n    calls += 1\n    return x * 2 + calls\n  }\n  return twice\n}\nfunc main() {\n  let f = curry(1)\n  let g = f(2)\n  print(g(3))\n  let t = maker()\n  print(t(5))\n  print(t(5))\n}"
        ),
        "123\n11\n12\n"
    );
}

#[test]
fn the_mutating_set_operations_change_the_set_in_place() {
    assert_eq!(
        main_printing(
            "var a: Set<Int> = [1, 2, 3]\nlet b: Set<Int> = [3, 4]\nlet copy = a\na.formUnion(b)\nprint(a.count)\nprint(copy.count)\na.formIntersection([2, 3, 4, 9])\nprint(a.count)\nprint(a.contains(2))\nprint(a.contains(1))\na.subtract([3])\nprint(a.count)\nprint(a.contains(3))\na.formSymmetricDifference([2, 7])\nprint(a.count)\nprint(a.contains(2))\nprint(a.contains(7))\nvar names: Set<String> = [\"x\"]\nnames.formUnion([\"y\", \"x\"])\nprint(names.count)"
        ),
        "4\n3\n3\ntrue\nfalse\n2\nfalse\n2\nfalse\ntrue\n2\n"
    );
}

#[test]
fn strict_subset_and_superset_require_a_smaller_or_larger_set() {
    assert_eq!(
        main_printing(
            "let a: Set<Int> = [1, 2]\nlet b: Set<Int> = [1, 2, 3]\nprint(a.isStrictSubset(of: b))\nprint(b.isStrictSubset(of: a))\nprint(a.isStrictSubset(of: a))\nprint(b.isStrictSuperset(of: a))\nprint(a.isStrictSuperset(of: b))\nprint(a.isStrictSuperset(of: a))\nlet e: Set<Int> = []\nprint(e.isStrictSubset(of: a))\nprint(e.isStrictSubset(of: e))"
        ),
        "true\nfalse\nfalse\ntrue\nfalse\nfalse\ntrue\nfalse\n"
    );
}

#[test]
fn the_mutating_set_operations_work_on_members_and_check_their_use() {
    assert_eq!(
        run(
            "class Group {\n  var members: Set<Int>\n  init() { self.members = [1] }\n  func join(_ others: Set<Int>) { members.formUnion(others) }\n}\nstruct Pair {\n  var left: Set<Int>\n}\nfunc main() {\n  let g = Group()\n  g.join([2, 3])\n  print(g.members.count)\n  var p = Pair(left: [1, 2])\n  p.left.subtract([1])\n  print(p.left.count)\n}"
        ),
        "3\n1\n"
    );
    for (body, message) in [
        ("let s: Set<Int> = [1]\n  s.formUnion([2])", "constant"),
        ("var s: Set<Int> = [1]\n  s.formUnion()", "formUnion"),
        ("var s: Set<Int> = [1]\n  s.formUnion([\"x\"])", "expected"),
        (
            "var s: Set<Int> = [1]\n  let t = s.subtract([1])",
            "statement",
        ),
        ("var a = [1]\n  a.formUnion([2])", "not a set"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_tuple_groups_values_and_is_read_by_position_and_by_pattern() {
    assert_eq!(
        run(
            "func divide(_ a: Int, _ b: Int) -> (Int, Int) {\n  return (a / b, a % b)\n}\nfunc main() {\n  let t = (1, \"one\")\n  print(t.0)\n  print(t.1)\n  let (q, r) = divide(17, 5)\n  print(q * 10 + r)\n  var v = (1, 2, 3)\n  v.1 = 20\n  let (a, _, c) = v\n  print(a + v.1 + c)\n  let nested = ((1, 2), 3)\n  print(nested.0.1 + nested.1)\n  let copy = v\n  v.0 = 9\n  print(copy.0)\n  print((1, \"a\") == (1, \"a\"))\n  print((1, 2) != (1, 3))\n  var swapped = (\"x\", 5)\n  swapped.0 += \"y\"\n  print(swapped.0)\n}"
        ),
        "1\none\n32\n24\n5\n1\ntrue\ntrue\nxy\n"
    );
}

#[test]
fn tuples_are_array_elements_dictionary_keys_and_closure_results() {
    assert_eq!(
        run(
            "func main() {\n  let pairs = [(1, \"a\"), (2, \"b\")]\n  print(pairs[1].1)\n  var total = 0\n  for (n, s) in pairs {\n    total += n + s.count\n  }\n  print(total)\n  var grid: [(Int, Int): String] = [:]\n  grid[(1, 2)] = \"x\"\n  print(grid[(1, 2)] ?? \"none\")\n  print(grid[(2, 1)] == nil)\n  let squares = [1, 2, 3].map { ($0, $0 * $0) }\n  print(squares[2].1)\n  let ordered = [(3, \"c\"), (1, \"a\")].sorted { $0.0 < $1.0 }\n  print(ordered[0].1)\n  let cases: [(Int, Int)] = [(1, 2)]\n  print(cases.count)\n}"
        ),
        "b\n5\nx\ntrue\n9\na\n1\n"
    );
}

#[test]
fn zip_and_enumerated_make_arrays_of_pairs() {
    assert_eq!(
        run(
            "func main() {\n  let names = [\"a\", \"b\", \"c\"]\n  let ages = [10, 20]\n  let z = zip(names, ages)\n  print(z.count)\n  print(z[1].0 + String(z[1].1))\n  for (n, a) in zip(names, ages) { print(n + String(a)) }\n  for (i, n) in names.enumerated() { print(String(i) + n) }\n  let e = names.enumerated()\n  print(e[2].0)\n  var total = 0\n  for (i, v) in [5, 6, 7].enumerated() { total += i * v }\n  print(total)\n}"
        ),
        "2\nb20\na10\nb20\n0a\n1b\n2c\n2\n20\n"
    );
}

#[test]
fn a_dictionary_is_built_from_an_array_of_pairs() {
    assert_eq!(
        run(
            "func main() {\n  let d = Dictionary(uniqueKeysWithValues: [(\"a\", 1), (\"b\", 2)])\n  print(d.count)\n  print(d[\"b\"] ?? 0)\n  let e = Dictionary(uniqueKeysWithValues: zip([1, 2, 3], [\"x\", \"y\", \"z\"]))\n  print(e[3] ?? \"\")\n  let f = Dictionary(uniqueKeysWithValues: [1, 2, 3].map { ($0, $0 * 10) })\n  print(f[2] ?? 0)\n  let g: [Int: Int] = Dictionary(uniqueKeysWithValues: [(1, 1)])\n  print(g.count)\n  let h = Dictionary(uniqueKeysWithValues: [(Int, Int)]())\n  print(h.count)\n}"
        ),
        "2\n2\nz\n20\n1\n0\n"
    );
}

#[test]
fn duplicate_keys_in_unique_keys_with_values_trap() {
    run_expecting_a_crash(
        "func main() {\n  let d = Dictionary(uniqueKeysWithValues: [(1, \"a\"), (1, \"b\")])\n  print(d.count)\n}",
    );
}

#[test]
fn tuple_misuse_is_rejected_with_a_message() {
    for (body, message) in [
        ("let t = (1, 2)\n  print(t.2)", "has no member '2'"),
        ("let (a, b) = 5", "is not a tuple"),
        (
            "let (a, b, c) = (1, 2)",
            "names 3 element(s), but the tuple has 2",
        ),
        ("let t = (1, 2)\n  t.0 = 3", "constant"),
        ("let t = (1, 2)\n  print(t == (1, \"a\"))", "expected"),
        (
            "let d = Dictionary(uniqueKeysWithValues: [1, 2])",
            "needs an array of pairs",
        ),
        (
            "let d = Dictionary(uniqueKeysWithValues: 3)",
            "needs an array of pairs",
        ),
        ("print(zip([1]))", "zip takes two sequences"),
        ("print(zip(1, 2))", "zip takes two sequences"),
        (
            "let xs = [1]\n  print(xs.enumerated(1))",
            "enumerated takes no arguments",
        ),
        (
            "for (a, b) in [1, 2] { }",
            "needs a dictionary or an array of pairs",
        ),
        ("let t = (1, 2, 3, 4, 5, 6)", "at most five elements"),
        (
            "let (a, b, c, d, e, f) = (1, 2, 3, 4, 5, 6)",
            "two to five elements",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_member_of_a_dictionary_entry_is_assigned_through_an_optional_chain() {
    assert_eq!(
        run(
            "struct Stats {\n  var count: Int\n  var name: String\n}\nclass Counter {\n  var n: Int\n}\nstruct Outer {\n  var inner: Stats\n}\nfunc main() {\n  var d = [\"a\": Stats(count: 1, name: \"x\")]\n  d[\"a\"]?.count += 5\n  d[\"a\"]?.name = \"renamed\"\n  d[\"missing\"]?.count = 99\n  print(d[\"a\"]?.count ?? -1)\n  print(d[\"a\"]?.name ?? \"\")\n  print(d.count)\n  let copy = d\n  d[\"a\"]?.count = 0\n  print(copy[\"a\"]?.count ?? -1)\n  var cs = [1: Counter(n: 1)]\n  cs[1]?.n += 10\n  cs[2]?.n = 5\n  print(cs[1]?.n ?? 0)\n  var os = [\"k\": Outer(inner: Stats(count: 0, name: \"i\"))]\n  os[\"k\"]?.inner.count = 7\n  print(os[\"k\"]?.inner.count ?? 0)\n}"
        ),
        "6\nrenamed\n1\n6\n11\n7\n"
    );
}

#[test]
fn a_collection_is_changed_through_an_optional_chain_only_when_the_entry_exists() {
    assert_eq!(
        run(
            "struct Team {\n  var members: [String]\n}\nfunc main() {\n  var groups = [\"a\": [1]]\n  groups[\"a\"]?.append(2)\n  groups[\"b\"]?.append(3)\n  print(groups[\"a\"]?.count ?? 0)\n  print(groups[\"b\"] == nil)\n  groups[\"a\"]?.removeLast()\n  groups[\"a\"]?.insert(7, at: 0)\n  print(groups[\"a\"]?[0] ?? 0)\n  var teams = [\"x\": Team(members: [\"p\"])]\n  teams[\"x\"]?.members.append(\"q\")\n  teams[\"y\"]?.members.append(\"z\")\n  print(teams[\"x\"]?.members.count ?? 0)\n  print(teams.count)\n}"
        ),
        "2\ntrue\n7\n2\n1\n"
    );
}

#[test]
fn a_member_of_an_optional_variable_or_member_is_assigned_through_a_chain() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var items: [Int]\n}\nclass Node {\n  var value: Int\n}\nclass Holder {\n  var p: P?\n  var node: Node?\n  init() {\n    self.p = P(x: 1, items: [])\n    self.node = nil\n  }\n}\nfunc main() {\n  var maybe: P? = P(x: 1, items: [])\n  maybe?.x = 5\n  maybe?.items.append(9)\n  print(maybe?.x ?? 0)\n  print(maybe?.items.count ?? 0)\n  var none: P? = nil\n  none?.x = 3\n  print(none == nil)\n  let h = Holder()\n  h.p?.x += 10\n  h.node?.value = 4\n  print(h.p?.x ?? 0)\n  print(h.node == nil)\n  let copy = maybe\n  maybe?.x = 0\n  print(copy?.x ?? -1)\n}"
        ),
        "5\n1\ntrue\n11\ntrue\n5\n"
    );
}

#[test]
fn an_optional_chain_can_subscript_its_value() {
    assert_eq!(
        main_printing(
            "let d = [\"a\": [10, 20]]\nprint(d[\"a\"]?[1] ?? -1)\nprint(d[\"z\"]?[0] ?? -1)\nlet maybe: [Int]? = [4]\nprint(maybe?[0] ?? -1)"
        ),
        "20\n-1\n4\n"
    );
}

#[test]
fn an_assignment_through_a_chain_is_checked() {
    for (body, message) in [
        ("var d = [1: 2]\n  d[1]?.foo = 3", "has no members"),
        ("var x = 5\n  x?.y = 1", "an assignment through '?.'"),
        ("let d = [\"a\": [1]]\n  d[\"a\"]?.append(1)", "constant"),
        ("var d = [1: (1, 2)]\n  d[1]?.0 = \"s\"", "expected Int"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_member_of_a_struct_is_assigned_through_a_default_entry() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var name: String\n}\nstruct Outer {\n  var inner: P\n  var tags: [String]\n}\nclass Counter {\n  var n: Int\n}\nfunc main() {\n  var d: [String: P] = [:]\n  d[\"a\", default: P(x: 0, name: \"\")].x = 5\n  d[\"a\", default: P(x: 0, name: \"\")].x += 1\n  d[\"b\", default: P(x: 10, name: \"n\")].name = \"m\"\n  print(d.count)\n  print(d[\"a\"]?.x ?? -1)\n  print(d[\"b\"]?.x ?? -1)\n  print(d[\"b\"]?.name ?? \"\")\n  let copy = d\n  d[\"a\", default: P(x: 0, name: \"\")].x = 0\n  print(copy[\"a\"]?.x ?? -1)\n  var os: [Int: Outer] = [:]\n  os[1, default: Outer(inner: P(x: 0, name: \"i\"), tags: [])].inner.x = 3\n  os[1, default: Outer(inner: P(x: 0, name: \"i\"), tags: [])].tags.append(\"t\")\n  print(os[1]?.inner.x ?? 0)\n  print(os[1]?.tags.count ?? 0)\n  var cs: [Int: Counter] = [:]\n  cs[1, default: Counter(n: 0)].n += 4\n  cs[1, default: Counter(n: 100)].n += 4\n  print(cs[1]?.n ?? 0)\n}"
        ),
        "2\n6\n10\nm\n6\n3\n1\n8\n"
    );
}

#[test]
fn a_default_entry_member_assignment_checks_its_use() {
    for (body, message) in [
        (
            "let d: [Int: (Int, Int)] = [:]\n  d[1, default: (0, 0)].0 = 1",
            "constant",
        ),
        (
            "var d: [Int: (Int, Int)] = [:]\n  d[1, default: (0, 0)].2 = 1",
            "has no member '2'",
        ),
        (
            "var d: [Int: (Int, Int)] = [:]\n  d[1, default: (0, 0)].0 = \"s\"",
            "expected Int",
        ),
        (
            "var d: [Int: (Int, Int)] = [:]\n  d[1, default: (0, 0, 0)].0 = 1",
            "expected",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_weak_capture_breaks_a_cycle_between_an_object_and_its_closure() {
    assert_eq!(
        run(
            "class Node {\n  var name: String\n  var onDone: (() -> Void)?\n  deinit { print(\"free \" + name) }\n  func setup() {\n    onDone = { [weak self] in\n      print(self?.name ?? \"gone\")\n    }\n  }\n}\nfunc main() {\n  let n = Node(name: \"a\", onDone: nil)\n  n.setup()\n  if let f = n.onDone { f() }\n  print(\"end\")\n}"
        ),
        "a\nend\nfree a\n"
    );
}

#[test]
fn a_weak_capture_becomes_nil_after_the_object_is_released() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(\"free\") }\n}\nfunc watch(_ b: Box) -> () -> Int {\n  return { [weak b] in\n    if let live = b { return live.n }\n    return -1\n  }\n}\nfunc main() {\n  var box: Box? = Box(n: 5)\n  var probe: (() -> Int)? = nil\n  if let b = box { probe = watch(b) }\n  if let p = probe { print(p()) }\n  box = nil\n  if let p = probe { print(p()) }\n  print(\"done\")\n}"
        ),
        "5\nfree\n-1\ndone\n"
    );
}

#[test]
fn an_unowned_capture_reads_the_object_without_keeping_it_alive() {
    assert_eq!(
        run(
            "class Owner {\n  var tag: Int\n  var action: (() -> Int)?\n  deinit { print(\"free\") }\n  func arm() {\n    action = { [unowned self] in self.tag * 2 }\n  }\n}\nfunc main() {\n  let o = Owner(tag: 21, action: nil)\n  o.arm()\n  if let a = o.action { print(a()) }\n  print(\"end\")\n}"
        ),
        "42\nend\nfree\n"
    );
}

#[test]
fn a_nested_closure_keeps_the_weak_reference_of_its_parent() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n  deinit { print(\"free\") }\n}\nfunc twice(_ f: () -> Int) -> Int { return f() + f() }\nfunc main() {\n  var box: Box? = Box(n: 4)\n  if let b = box {\n    let sum = twice { [weak b] in\n      twice { b?.n ?? 0 }\n    }\n    print(sum)\n  }\n  box = nil\n  print(\"done\")\n}"
        ),
        "16\nfree\ndone\n"
    );
}

#[test]
fn creating_closures_with_weak_captures_in_a_loop_does_not_leak() {
    let source = "class Box {\n  var n: Int\n}\nfunc main() {\n  var total = 0\n  var i = 0\n  while i < 200000 {\n    let b = Box(n: i)\n    let f = { [weak b] in b?.n ?? 0 }\n    let g = { [unowned b] in b.n }\n    total += f() + g()\n    i += 1\n  }\n  print(total)\n}";
    assert_eq!(run(source), "39999800000\n");
}

#[test]
fn a_capture_list_is_checked() {
    let class = "class C {\n  var v: Int\n}\n";
    for (prelude, body, message) in [
        (
            "",
            "var n = 1\n  let f = { [weak n] in n }",
            "cannot be captured weakly or unowned",
        ),
        (
            "",
            "let f = { [weak ghost] in 1 }",
            "cannot find 'ghost' in scope",
        ),
        (
            class,
            "let c = C(v: 1)\n  let f: () -> Void = { [weak c] in\n    c = nil\n  }",
            "constant",
        ),
        (
            class,
            "let c = C(v: 1)\n  let f = { [weak c] in c.v }",
            "no members",
        ),
    ] {
        let source = format!("{prelude}func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_capture_list_entry_copies_the_value_at_the_moment_the_closure_is_made() {
    assert_eq!(
        main_printing(
            "var n = 1\nlet byReference = { n }\nlet byCopy = { [n] in n }\nn = 5\nprint(byReference())\nprint(byCopy())\nvar a = [1]\nlet size = { [a] in a.count }\na.append(2)\nprint(size())\nprint(a.count)\nvar s = \"x\"\nlet text = { [s] in s + \"!\" }\ns += \"y\"\nprint(text())"
        ),
        "5\n1\n1\n2\nx!\n"
    );
}

#[test]
fn a_capture_list_entry_may_compute_its_value_and_a_struct_is_copied() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nclass Box {\n  var n: Int\n}\nfunc main() {\n  var p = P(x: 1)\n  let snapshot = { [p] in p.x }\n  p.x = 9\n  print(snapshot())\n  let base = 10\n  let doubled = { [twice = base * 2] in twice + 1 }\n  print(doubled())\n  let box = Box(n: 1)\n  let same = { [box] in box.n }\n  box.n = 7\n  print(same())\n  var fs: [() -> Int] = []\n  for i in 0..<3 {\n    fs.append({ [i] in i * 10 })\n  }\n  var total = 0\n  for f in fs { total += f() }\n  print(total)\n}"
        ),
        "1\n21\n7\n30\n"
    );
}

#[test]
fn a_copied_capture_is_a_constant_and_may_be_given_to_a_nested_closure() {
    assert_eq!(
        run(
            "func twice(_ f: () -> Int) -> Int { return f() + f() }\nfunc main() {\n  var n = 3\n  let r = twice { [n] in twice { n } }\n  n = 100\n  print(r)\n}"
        ),
        "12\n"
    );
    assert!(
        build_error("func main() {\n  var n = 1\n  let f: () -> Void = { [n] in n += 1 }\n}")
            .contains("constant")
    );
    assert!(
        build_error("func main() {\n  let f = { [x = nope] in x }\n}")
            .contains("cannot find 'nope'")
    );
    assert!(
        build_error("func main() {\n  let f = { [weak x = 5] in x }\n}")
            .contains("cannot be captured weakly or unowned")
    );
}

#[test]
fn a_weak_capture_may_name_the_value_it_holds() {
    assert_eq!(
        run(
            "class Box {\n  var n: Int\n}\nfunc main() {\n  var box: Box? = Box(n: 6)\n  if let b = box {\n    let f = { [weak held = b] in held?.n ?? -1 }\n    print(f())\n    box = nil\n    print(f())\n  }\n}"
        ),
        "6\n6\n"
    );
}

#[test]
fn a_tuple_member_is_read_and_assigned_by_label() {
    assert_eq!(
        run(
            "func bounds(_ xs: [Int]) -> (min: Int, max: Int) {\n  var lo = xs[0]\n  var hi = xs[0]\n  for x in xs {\n    if x < lo { lo = x }\n    if x > hi { hi = x }\n  }\n  return (min: lo, max: hi)\n}\nfunc main() {\n  let r = bounds([3, 9, 1, 4])\n  print(r.min)\n  print(r.max)\n  print(r.0 + r.1)\n  var p = (x: 1, y: 2)\n  p.x = 10\n  p.y += 5\n  print(p.x * 100 + p.y)\n  let (a, b) = p\n  print(a + b)\n  let named: (id: Int, name: String) = (id: 7, name: \"seven\")\n  print(named.name)\n  for (i, n) in [\"a\", \"b\"].enumerated() { print(String(i) + n) }\n  let e = [5, 6].enumerated()\n  print(e[1].offset + e[1].element)\n}"
        ),
        "1\n9\n10\n1007\n17\nseven\n0a\n1b\n7\n"
    );
}

#[test]
fn labeled_and_unlabeled_tuples_convert_where_the_labels_agree() {
    assert_eq!(
        run(
            "func pair() -> (Int, Int) { return (x: 1, y: 2) }\nfunc named(_ p: (a: Int, b: Int)) -> Int { return p.a * 10 + p.b }\nfunc main() {\n  let plain: (Int, Int) = (x: 3, y: 4)\n  print(plain.0 + plain.1)\n  let labeled: (x: Int, y: Int) = (5, 6)\n  print(labeled.y)\n  let p = pair()\n  print(p.1)\n  print(named((7, 8)))\n  print(named((a: 1, b: 2)))\n  let u = (1, 2)\n  let again: (first: Int, second: Int) = u\n  print(again.second)\n  print((x: 1, y: 2) == (1, 2))\n}"
        ),
        "7\n6\n2\n78\n12\n2\ntrue\n"
    );
}

#[test]
fn tuple_labels_are_checked() {
    for (body, message) in [
        ("let t = (x: 1, y: 2)\n  print(t.z)", "has no member 'z'"),
        (
            "let t: (a: Int, b: Int) = (x: 1, y: 2)",
            "does not match the expected label",
        ),
        ("let t = (x: 1)", "at least two elements"),
        (
            "let u: (a: Int, b: Int) = (1, 2)\n  let v: (c: Int, d: Int) = u",
            "expected",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_tuple_of_places_is_assigned_from_a_tuple_at_once() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var y: Int\n}\nfunc fib(_ n: Int) -> Int {\n  var (a, b) = (0, 1)\n  var i = 0\n  while i < n {\n    (a, b) = (b, a + b)\n    i += 1\n  }\n  return a\n}\nfunc main() {\n  var a = 1\n  var b = 2\n  (a, b) = (b, a)\n  print(a * 10 + b)\n  print(fib(10))\n  var p = P(x: 1, y: 2)\n  (p.x, p.y) = (p.y, p.x)\n  print(p.x * 10 + p.y)\n  var xs = [1, 2, 3]\n  (xs[0], xs[2]) = (xs[2], xs[0])\n  print(xs[0] * 100 + xs[1] * 10 + xs[2])\n  var s = \"a\"\n  var n = 0\n  (s, _) = (\"b\", 5)\n  (n, _) = (7, \"ignored\")\n  print(s)\n  print(n)\n  let pair = (4, 5)\n  (a, b) = pair\n  print(a + b)\n}"
        ),
        "21\n55\n21\n321\nb\n7\n9\n"
    );
}

#[test]
fn a_tuple_assignment_is_checked() {
    for (body, message) in [
        ("var a = 1\n  var b = 2\n  (a, b) = 5", "is not a tuple"),
        (
            "var a = 1\n  var b = 2\n  (a, b) = (1, 2, 3)",
            "assigns 2 element(s), but the tuple has 3",
        ),
        (
            "var a = 1\n  var b = 2\n  (a, b) += (1, 2)",
            "compound operator",
        ),
        ("let a = 1\n  var b = 2\n  (a, b) = (3, 4)", "constant"),
        (
            "var a = 1\n  var b = 2\n  (a, b) = (\"s\", 2)",
            "expected Int",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_switch_matches_the_elements_of_a_tuple() {
    assert_eq!(
        run(
            "func describe(_ p: (Int, Int)) -> String {\n  switch p {\n  case (0, 0):\n    return \"origin\"\n  case (0, _):\n    return \"on y\"\n  case (_, 0):\n    return \"on x\"\n  case (let a, let b):\n    return \"at \" + String(a) + \",\" + String(b)\n  }\n}\nfunc main() {\n  print(describe((0, 0)))\n  print(describe((0, 5)))\n  print(describe((3, 0)))\n  print(describe((2, 7)))\n}"
        ),
        "origin\non y\non x\nat 2,7\n"
    );
}

#[test]
fn a_tuple_switch_takes_values_ranges_alternatives_strings_and_a_default() {
    assert_eq!(
        run(
            "func grade(_ score: Int, _ pass: Bool) -> String {\n  switch (score, pass) {\n  case (90...100, true):\n    return \"A\"\n  case (60..<90, true), (95, false):\n    return \"B\"\n  default:\n    return \"F\"\n  }\n}\nfunc kind(_ name: String, _ n: Int) -> String {\n  switch (name, n) {\n  case (\"x\", _):\n    return \"x\"\n  case (_, -1):\n    return \"minus\"\n  case (\"y\", let m):\n    return \"y\" + String(m)\n  default:\n    return \"none\"\n  }\n}\nfunc main() {\n  print(grade(95, true))\n  print(grade(70, true))\n  print(grade(95, false))\n  print(grade(30, true))\n  print(kind(\"x\", 5))\n  print(kind(\"z\", -1))\n  print(kind(\"y\", 4))\n  print(kind(\"q\", 4))\n  var found = 0\n  for (i, v) in [3, 0, 5].enumerated() {\n    switch (i, v) {\n    case (_, 0):\n      found = i\n    default:\n      break\n    }\n  }\n  print(found)\n}"
        ),
        "A\nB\nB\nF\nx\nminus\ny4\nnone\n1\n"
    );
}

#[test]
fn a_tuple_switch_is_checked() {
    for (body, message) in [
        (
            "switch (1, 2) {\n  case (1, _):\n    break\n  }",
            "needs a 'default' or a last case that matches every tuple",
        ),
        (
            "switch (1, 2) {\n  case (1, _, _):\n    break\n  default:\n    break\n  }",
            "has 3 element(s), but the tuple has 2",
        ),
        (
            "switch (1, 2) {\n  case 1:\n    break\n  default:\n    break\n  }",
            "needs a tuple pattern",
        ),
        (
            "switch (1, 2) {\n  case (1, \"a\"):\n    break\n  default:\n    break\n  }",
            "expected Int",
        ),
        (
            "switch (1, 2) {\n  case (let a, 1), (2, 2):\n    print(a)\n  default:\n    break\n  }",
            "cannot bind names",
        ),
        (
            "switch 5 {\n  case (1, 2):\n    break\n  default:\n    break\n  }",
            "cannot match",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_tuple_is_printed_and_converted_to_a_string_like_swift_does() {
    assert_eq!(
        run(
            "enum Color {\n  case red\n}\nfunc main() {\n  print((1, 2))\n  print((1, \"a\"))\n  print((x: 3, y: true))\n  let c: Character = \"c\"\n  print((c, Color.red))\n  let t = (1, (2, 3))\n  print(t)\n  let s = String((4, \"z\"))\n  print(s + \"!\")\n  print(\"pair \\((5, 6))\")\n  print([(1, \"a\"), (2, \"b\")][1])\n  print((offset: 0, element: \"q\"))\n}"
        ),
        "(1, 2)\n(1, \"a\")\n(x: 3, y: true)\n(\"c\", red)\n(1, (2, 3))\n(4, \"z\")!\npair (5, 6)\n(2, \"b\")\n(offset: 0, element: \"q\")\n"
    );
}

#[test]
fn a_tuple_that_holds_an_unprintable_value_is_rejected() {
    assert!(
        build_error("class C {}\nfunc main() {\n  print((1, C()))\n}")
            .contains("cannot print a tuple that holds")
    );
}

#[test]
fn if_case_tests_one_pattern_and_binds_its_names() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case square(Int)\n  case empty\n}\nfunc area(_ s: Shape) -> Int {\n  if case .circle(let r) = s {\n    return r * 3\n  } else if case .square(let w) = s {\n    return w * w\n  }\n  return 0\n}\nfunc main() {\n  print(area(.circle(4)))\n  print(area(Shape.square(5)))\n  print(area(.empty))\n  if case .circle(let r) = Shape.circle(7) {\n    print(r)\n  } else {\n    print(\"no\")\n  }\n  if case .square(_) = Shape.circle(1) {\n    print(\"square\")\n  } else {\n    print(\"not a square\")\n  }\n  let maybe: Int? = 9\n  if case .some(let v) = maybe { print(v) }\n  let none: Int? = nil\n  if case .none = none { print(\"nil\") }\n}"
        ),
        "12\n25\n0\n7\nnot a square\n9\nnil\n"
    );
}

#[test]
fn if_case_matches_tuples_values_and_ranges() {
    assert_eq!(
        main_printing(
            "let t = (0, 5)\nif case (0, let y) = t { print(y) }\nif case (1, _) = t { print(\"one\") } else { print(\"not one\") }\nlet n = 3\nif case 1...5 = n { print(\"in\") }\nif case 10 = n { print(\"ten\") } else { print(\"not ten\") }\nlet s = \"go\"\nif case \"go\" = s { print(\"matched\") }\nvar total = 0\nfor i in 0..<6 {\n  if case 2 = i { continue }\n  if case 4 = i { break }\n  total += i\n}\nprint(total)"
        ),
        "5\nnot one\nin\nnot ten\nmatched\n4\n"
    );
}

#[test]
fn if_case_bindings_do_not_outlive_the_statement_and_nested_ones_work() {
    assert_eq!(
        run(
            "enum Pair {\n  case two(Int, Int)\n  case one(Int)\n}\nfunc main() {\n  let p = Pair.two(3, 4)\n  let r = 100\n  if case .two(let a, let b) = p {\n    if case .one(let c) = Pair.one(a + b) {\n      print(c + r)\n    }\n  }\n  print(r)\n}"
        ),
        "107\n100\n"
    );
    assert!(
        build_error("enum E {\n  case a(Int)\n}\nfunc main() {\n  if case .a(let x) = E.a(1) { }\n  print(x)\n}")
            .contains("cannot find 'x'")
    );
}

#[test]
fn if_case_is_checked() {
    for (prelude, body, message) in [
        ("", "if case 1 = \"a\" { }", "expected"),
        ("", "if case .nope = Optional<Int>.none { }", "nope"),
        (
            "enum E {\n  case a\n}\n",
            "if case 5 = E.a { }",
            "cannot be a value",
        ),
        (
            "",
            "if case (1, 2, 3) = (1, 2) { }",
            "has 3 element(s), but the tuple has 2",
        ),
    ] {
        let source = format!("{prelude}func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn an_if_takes_several_conditions_of_every_kind() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case square(Int)\n}\nfunc describe(_ s: Shape?, _ limit: Int) -> String {\n  if let shape = s, case .circle(let r) = shape, r > limit {\n    return \"big circle \" + String(r)\n  } else if let shape = s, case .square(let w) = shape {\n    return \"square \" + String(w)\n  } else {\n    return \"other\"\n  }\n}\nfunc main() {\n  print(describe(.circle(9), 5))\n  print(describe(.circle(2), 5))\n  print(describe(.square(4), 5))\n  print(describe(nil, 5))\n  let a = 3\n  let b = 4\n  if a > 1, b > 3 { print(\"both\") }\n  if a > 1, b > 9 { print(\"no\") } else { print(\"else ran once\") }\n  let pair = (1, 2)\n  let maybe: Int? = 5\n  if case (1, let y) = pair, let m = maybe, y + m == 7 { print(\"sum \" + String(y + m)) }\n}"
        ),
        "big circle 9\nother\nsquare 4\nother\nboth\nelse ran once\nsum 7\n"
    );
}

#[test]
fn a_guard_binds_names_for_the_rest_of_the_scope_from_every_kind_of_condition() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case other\n}\nfunc radius(_ s: Shape, _ extra: Int?) -> Int {\n  guard case .circle(let r) = s, let e = extra, r > 0 else {\n    return -1\n  }\n  return r + e\n}\nfunc corner(_ p: (Int, Int)) -> String {\n  guard case (0, let y) = p else {\n    return \"away\"\n  }\n  return \"y=\" + String(y)\n}\nfunc main() {\n  print(radius(.circle(5), 2))\n  print(radius(.circle(0), 2))\n  print(radius(.other, 2))\n  print(radius(.circle(5), nil))\n  print(corner((0, 7)))\n  print(corner((1, 7)))\n  var total = 0\n  for i in 0..<5 {\n    guard case 0...2 = i, i != 1 else { continue }\n    total += i\n  }\n  print(total)\n}"
        ),
        "7\n-1\n-1\n-1\ny=7\naway\n2\n"
    );
}

#[test]
fn a_while_loops_while_every_condition_holds_and_re_evaluates_them() {
    assert_eq!(
        run(
            "enum Step {\n  case go(Int)\n  case stop\n}\nfunc next(_ n: Int) -> Step {\n  if n < 4 { return .go(n + 1) }\n  return .stop\n}\nfunc main() {\n  var n = 0\n  var seen = 0\n  while case .go(let m) = next(n) {\n    n = m\n    seen += 1\n  }\n  print(n)\n  print(seen)\n  var xs = [1, 2, 3, 0, 5]\n  var sum = 0\n  while let first = xs.first, first > 0 {\n    sum += first\n    xs.remove(at: 0)\n  }\n  print(sum)\n  print(xs.count)\n  var i = 0\n  var evens = 0\n  while i < 10, case 0...7 = i {\n    i += 1\n    if i % 2 == 1 { continue }\n    evens += 1\n    if evens == 3 { break }\n  }\n  print(i)\n  print(evens)\n}"
        ),
        "4\n4\n6\n2\n6\n3\n"
    );
}

#[test]
fn a_labeled_loop_with_conditions_can_be_left_from_an_inner_loop() {
    assert_eq!(
        main_printing(
            "var found = 0\nouter: while found < 100, case 0...99 = found {\n  var j = 0\n  while j < 5 {\n    j += 1\n    found += 10\n    if found >= 30 { break outer }\n  }\n}\nprint(found)"
        ),
        "30\n"
    );
}

#[test]
fn condition_lists_are_checked() {
    for (body, message) in [
        ("if 1 { }", "expected Bool"),
        ("let x = 1\n  if let v = x, v > 0 { }", "needs an optional"),
        (
            "let x = 1\n  guard case 1 = x, x > 0 else {\n    print(x)\n  }",
            "must leave the scope",
        ),
        ("while case .none = 5, true { }", "cannot match"),
        (
            "let t = (1, 2)\n  guard case (let a, let b) = t, case (0, _), 3 = t else { return }",
            "expected",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_let_before_the_pattern_binds_every_payload_name_in_a_switch_an_if_and_a_guard() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case rect(width: Int, height: Int)\n  case dot\n}\nfunc area(_ s: Shape) -> Int {\n  switch s {\n  case let .circle(r): return r * r\n  case let .rect(w, h): return w * h\n  case .dot: return 0\n  }\n}\nfunc radius(_ s: Shape) -> Int {\n  guard case let .circle(r) = s else { return -1 }\n  return r\n}\nfunc main() {\n  print(area(.circle(3)))\n  print(area(.rect(width: 2, height: 5)))\n  print(area(.dot))\n  print(radius(.circle(4)))\n  print(radius(.dot))\n  let o: Int? = 8\n  if case let .some(v) = o, v > 1 { print(v) }\n  switch Shape.rect(width: 1, height: 2) {\n  case let .rect(w, _): print(w)\n  default: print(0)\n  }\n  let b: Shape = .circle(6)\n  switch b {\n  case let .circle(r), .rect(width: let r, height: _): print(r)\n  default: print(0)\n  }\n}"
        ),
        "9\n10\n0\n4\n-1\n8\n1\n6\n"
    );
}

#[test]
fn a_let_before_the_pattern_is_checked_like_a_let_in_it() {
    for (body, message) in [
        (
            "let o: Int? = 1\n  switch o {\n  case let .some(v), .none: print(v)\n  }",
            "must bind the same names",
        ),
        (
            "let o: Int? = 1\n  if case let .some(let v) = o { print(v) }",
            "expected",
        ),
        (
            "let o: Int? = 1\n  if case let .some(v, w) = o { print(v) }",
            "carries 1 value",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_where_clause_skips_a_case_whose_condition_fails_and_falls_to_the_next_case() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case rect(Int, Int)\n  case dot\n}\nfunc label(_ s: Shape) -> String {\n  switch s {\n  case .circle(let r) where r > 10: return \"big circle\"\n  case .circle(let r) where r > 5: return \"medium circle\"\n  case .circle: return \"small circle\"\n  case .rect(let w, let h) where w == h: return \"square\"\n  case .rect(let w, _) where w > 100: return \"wide\"\n  case .rect: return \"rect\"\n  case .dot: return \"dot\"\n  }\n}\nfunc main() {\n  print(label(.circle(20)))\n  print(label(.circle(7)))\n  print(label(.circle(1)))\n  print(label(.rect(3, 3)))\n  print(label(.rect(200, 1)))\n  print(label(.rect(2, 1)))\n  print(label(.dot))\n}"
        ),
        "big circle\nmedium circle\nsmall circle\nsquare\nwide\nrect\ndot\n"
    );
}

#[test]
fn a_where_clause_on_values_and_tuples_and_with_a_default() {
    assert_eq!(
        run(
            "func kind(_ n: Int, _ even: Bool) -> String {\n  switch n {\n  case 0...9 where even: return \"small even\"\n  case 0...9: return \"small\"\n  case 10, 11 where !even: return \"ten or eleven odd\"\n  default: return \"large\"\n  }\n}\nfunc quadrant(_ p: (Int, Int)) -> String {\n  switch p {\n  case (let x, let y) where x == y: return \"diagonal\"\n  case (0, _), (_, 0) where p.0 + p.1 > 5: return \"far axis\"\n  case (let x, _) where x < 0: return \"left\"\n  default: return \"other\"\n  }\n}\nfunc main() {\n  print(kind(4, true))\n  print(kind(4, false))\n  print(kind(10, false))\n  print(kind(12, true))\n  print(quadrant((2, 2)))\n  print(quadrant((0, 1)))\n  print(quadrant((9, 0)))\n  print(quadrant((1, 0)))\n  print(quadrant((-3, 5)))\n}"
        ),
        "small even\nsmall\nten or eleven odd\nlarge\ndiagonal\nfar axis\nfar axis\nother\nleft\n"
    );
}

#[test]
fn a_where_clause_binds_only_the_last_pattern_of_a_case_and_can_leave_through_break() {
    assert_eq!(
        run(
            "enum E {\n  case a(Int)\n  case b(Int)\n}\nfunc main() {\n  for e in [E.a(1), E.b(1), E.b(9)] {\n    switch e {\n    case let .a(n), .b(let n) where n > 5:\n      print(\"hit \" + String(n))\n    case .a, .b:\n      print(\"miss\")\n    }\n  }\n  var total = 0\n  for i in 0..<6 {\n    switch i {\n    case 0...5 where i > 3: break\n    default: total += i\n    }\n  }\n  print(total)\n}"
        ),
        "hit 1\nmiss\nhit 9\n6\n"
    );
}

#[test]
fn a_where_clause_does_not_make_a_switch_exhaustive_and_must_be_a_bool() {
    for (body, message) in [
        (
            "let o: Int? = 1\n  switch o {\n  case .some(let v) where v > 0: print(v)\n  case .none: print(0)\n  }",
            "missing case 'some'",
        ),
        (
            "let b = true\n  switch b {\n  case true where b: print(1)\n  case false: print(0)\n  }",
            "missing case 'true'",
        ),
        (
            "let o: Int? = 1\n  switch o {\n  case .some(let v) where v: print(v)\n  default: print(0)\n  }",
            "expected Bool",
        ),
        (
            "let t = (1, 2)\n  switch t {\n  case (let a, let b) where a < b: print(a)\n  }",
            "needs a 'default'",
        ),
        (
            "let o: Int? = 1\n  switch o {\n  case .some(let v) where w > 0: print(v)\n  default: print(0)\n  }",
            "w",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn zip_pairs_a_range_a_string_a_set_and_a_dictionary_with_any_sequence() {
    assert_eq!(
        run(
            "func main() {\n  let names = [\"a\", \"b\", \"c\"]\n  for (i, n) in zip(0..<3, names) { print(String(i) + n) }\n  for (n, i) in zip(names, 1...2) { print(n + String(i)) }\n  for (c, k) in zip(\"xyz\", [10, 20]) { print(String(c) + String(k)) }\n  let s: Set<Int> = [7]\n  for (v, w) in zip(s, \"q\") { print(String(v) + String(w)) }\n  let d = [\"k\": 1]\n  for (pair, flag) in zip(d, [true]) { print(pair.0 + String(pair.1) + String(flag)) }\n  let lo = 2\n  let pairs = zip(lo..<5, lo...6)\n  print(pairs.count)\n  print(pairs[2].0 + pairs[2].1)\n  print(zip(\"ab\", \"cd\").count)\n  print(zip(0..<0, names).count)\n}"
        ),
        "0a\n1b\n2c\na1\nb2\nx10\ny20\n7q\nk1true\n3\n8\n2\n0\n"
    );
}

#[test]
fn zip_rejects_what_is_not_a_sequence() {
    for (body, message) in [
        ("print(zip(1, [2]).count)", "zip takes two sequences"),
        ("print(zip([1], true).count)", "zip takes two sequences"),
        ("print(zip(0..<\"a\", [1]).count)", "expected Int"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_tuple_pattern_nests_tuples_and_enum_patterns_with_their_own_let() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case square(Int)\n}\nfunc nested(_ t: ((Int, Int), String)) -> String {\n  switch t {\n  case ((0, let y), _): return \"origin column \" + String(y)\n  case ((let a, let b), \"x\") where a == b: return \"diagonal x\"\n  case (_, let s): return s\n  }\n}\nfunc pair(_ t: (Shape, Int)) -> Int {\n  switch t {\n  case (.circle(let r), let n): return r * n\n  case (.square(let w), 0...3): return w\n  case (.square, _): return -1\n  default: return 0\n  }\n}\nfunc main() {\n  print(nested(((0, 5), \"q\")))\n  print(nested(((2, 2), \"x\")))\n  print(nested(((2, 3), \"x\")))\n  print(pair((.circle(3), 4)))\n  print(pair((.square(7), 2)))\n  print(pair((Shape.square(7), 9)))\n  if case (.circle(let r), (1, let k)) = (Shape.circle(5), (1, 8)) { print(r + k) }\n  if case ((_, _), .square(let w)) = ((1, 2), Shape.square(6)) { print(w) } else { print(\"no\") }\n}"
        ),
        "origin column 5\ndiagonal x\nx\n12\n7\n-1\n13\n6\n"
    );
}

#[test]
fn nested_tuple_patterns_are_checked() {
    for (body, message) in [
        (
            "let t = (1, 2)\n  switch t {\n  case ((let a, let b), _): print(a)\n  default: print(0)\n  }",
            "cannot match",
        ),
        (
            "let t = ((1, 2), 3)\n  switch t {\n  case ((let a, _), _), (_, let a): print(a)\n  default: print(0)\n  }",
            "cannot bind names",
        ),
        (
            "let t = ((1, 2), 3)\n  switch t {\n  case ((let a, _, _), _): print(a)\n  default: print(0)\n  }",
            "element(s)",
        ),
        (
            "let t = (1, 2)\n  switch t {\n  case (.some(let a), _): print(a)\n  default: print(0)\n  }",
            "cannot match",
        ),
        (
            "let t = ((1, 2), 3)\n  switch t {\n  case ((let a, _), let a): print(a)\n  default: print(0)\n  }",
            "a",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        let error = build_error(&source);
        assert!(error.contains(message), "{body}: {error}");
    }
}

#[test]
fn a_where_clause_filters_the_elements_of_a_for_loop_and_continue_still_works() {
    assert_eq!(
        run(
            "func main() {\n  var total = 0\n  for i in 0..<10 where i % 2 == 0 { total += i }\n  print(total)\n  for n in [3, 8, 5, 12] where n > 4 {\n    if n == 5 { continue }\n    print(n)\n  }\n  for c in \"a-b\" where c != \"-\" { print(c) }\n  let d = [\"x\": 1, \"y\": 2]\n  for (k, v) in d where v > 1 { print(k) }\n  var seen = 0\n  outer: for i in 0..<5 where i > 0 {\n    for j in 0..<5 where j == i {\n      seen += j\n      if seen > 5 { break outer }\n    }\n  }\n  print(seen)\n}"
        ),
        "20\n8\n12\na\nb\ny\n6\n"
    );
}

#[test]
fn a_where_clause_after_if_case_guard_case_and_while_case_adds_a_condition() {
    assert_eq!(
        run(
            "enum Shape {\n  case circle(Int)\n  case dot\n}\nfunc big(_ s: Shape) -> String {\n  if case .circle(let r) = s where r > 5 { return \"big\" } else { return \"not big\" }\n}\nfunc radius(_ s: Shape) -> Int {\n  guard case .circle(let r) = s where r > 0 else { return -1 }\n  return r\n}\nfunc main() {\n  print(big(.circle(9)))\n  print(big(.circle(2)))\n  print(big(.dot))\n  print(radius(.circle(4)))\n  print(radius(.circle(0)))\n  var queue = [1, 2, 5]\n  var sum = 0\n  while case .some(let r) = queue.first where r < 3 {\n    sum += r\n    queue.remove(at: 0)\n  }\n  print(sum)\n}"
        ),
        "big\nnot big\nnot big\n4\n-1\n3\n"
    );
}

#[test]
fn a_tuple_holds_up_to_five_elements() {
    assert_eq!(
        run(
            "func make(_ n: Int) -> (Int, String, Bool, Character, Int) {\n  return (n, \"s\", n > 1, \"c\", n * 2)\n}\nfunc main() {\n  let t = make(3)\n  print(t.0 + t.4)\n  let (a, b, c, d, e) = t\n  print(b + String(d) + String(c) + String(a + e))\n  var u = (1, 2, 3, 4, 5)\n  u.4 += 10\n  (u.0, u.1) = (u.1, u.0)\n  print(u)\n  print(u == (2, 1, 3, 4, 15))\n  let named = (a: 1, b: 2, c: 3, d: 4, e: 5)\n  print(named.e)\n  switch u {\n  case (2, _, _, _, let last): print(last)\n  default: print(0)\n  }\n  var seen: Set<Int> = []\n  for v in [(1, 2, 3, 4, 5), (1, 2, 3, 4, 5)] { seen.insert(v.2) }\n  print(seen.count)\n}"
        ),
        "9\nsctrue9\n(2, 1, 3, 4, 15)\ntrue\n5\n15\n1\n"
    );
}

#[test]
fn parentheses_around_one_name_group_it_because_there_is_no_one_element_tuple() {
    assert_eq!(
        main_printing(
            "let (a) = 4\nvar (b) = (a + 1)\nb += 1\nlet c: (Int) = (b)\nprint(a + b + c)"
        ),
        "16\n"
    );
}

#[test]
fn local_functions_may_call_each_other_before_the_later_one_is_declared() {
    assert_eq!(
        main_printing(
            "var calls = 0\nfunc isEven(_ n: Int) -> Bool {\n  calls += 1\n  if n == 0 { return true }\n  return isOdd(n - 1)\n}\nfunc isOdd(_ n: Int) -> Bool {\n  if n == 0 { return false }\n  return isEven(n - 1)\n}\nprint(isEven(10))\nprint(isOdd(7))\nprint(isEven(7))\nprint(calls)"
        ),
        "true\ntrue\nfalse\n14\n"
    );
}

#[test]
fn a_local_function_called_before_a_function_it_needs_is_declared_is_rejected() {
    let source = "func main() {\n  func a() -> Int { return b() }\n  print(a())\n  func b() -> Int { return 1 }\n}";
    assert!(build_error(source).contains("before"));
}

#[test]
fn a_local_function_may_have_type_parameters_inferred_at_each_call() {
    assert_eq!(
        main_printing(
            "var calls = 0\nfunc twice<T>(_ x: T, _ f: (T) -> T) -> T {\n  calls += 1\n  return f(f(x))\n}\nfunc pick<A, B>(_ a: A, _ b: B) -> B { return b }\nprint(twice(3) { $0 + 1 })\nprint(twice(\"a\") { $0 + \"b\" })\nprint(pick(1, \"two\"))\nprint(pick(true, [1, 2]).count)\nprint(calls)"
        ),
        "5\nabb\ntwo\n2\n2\n"
    );
}

#[test]
fn a_generic_local_function_may_recurse_use_bounds_and_be_called_from_a_closure() {
    assert_eq!(
        main_printing(
            "let base = 10\nfunc largest<T: Comparable>(_ xs: [T]) -> T {\n  var best = xs[0]\n  for x in xs { if x > best { best = x } }\n  return best\n}\nfunc depth<T>(_ x: T, _ n: Int) -> Int {\n  if n == 0 { return base }\n  return depth(x, n - 1) + 1\n}\nlet viaClosure = { (n: Int) -> Int in largest([n, base, 3]) }\nprint(largest([3, 9, 2]))\nprint(largest([\"b\", \"z\", \"a\"]))\nprint(depth(\"x\", 3))\nprint(viaClosure(42))"
        ),
        "9\nz\n13\n42\n"
    );
}

#[test]
fn a_generic_local_function_is_checked_against_its_bounds_and_type_parameters() {
    for (body, message) in [
        (
            "func big<T: Comparable>(_ a: T) -> T { return a }\n  let r = big([1])",
            "does not conform",
        ),
        (
            "func none<T>() -> T? { return nil }\n  let v = none()",
            "cannot infer the type parameter",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        assert!(build_error(&source).contains(message), "{body}");
    }
}

#[test]
fn a_generic_local_function_works_inside_a_generic_function_and_with_explicit_arguments() {
    assert_eq!(
        run(
            "func wrap<T>(_ x: T) -> [T] {\n  func pair<U>(_ a: T, _ b: U) -> [T] { return [a] }\n  func noop() { }\n  func empty<U>() -> [U] { return [] }\n  noop()\n  let e: [Int] = empty<Int>()\n  return pair(x, e.count) + pair(x, \"s\")\n}\nfunc main() {\n  print(wrap(7).count)\n  print(wrap(\"a\")[1])\n}"
        ),
        "2\na\n"
    );
}

#[test]
fn generic_local_functions_may_call_each_other_through_a_closure() {
    assert_eq!(
        main_printing(
            "var seed = 5\nfunc inner<T>(_ x: T) -> Int { return seed }\nfunc outer<T>(_ x: T) -> Int { return inner(x) + 1 }\nlet f = { outer(\"s\") }\nseed = 7\nprint(f())"
        ),
        "8\n"
    );
}

#[test]
fn a_generic_local_function_checks_the_number_of_arguments() {
    let error =
        build_error("func main() {\n  func f<T>(_ a: T) -> T { return a }\n  let x = f(1, 2)\n}");
    assert!(error.contains("takes 1 argument(s), but 2 were given"));
}

#[test]
fn an_array_takes_prefixes_suffixes_drops_and_reverses_into_new_arrays() {
    assert_eq!(
        main_printing(
            "func show(_ xs: [Int]) {\n  print(xs.map { String($0) }.joined(separator: \",\"))\n}\nlet a = [1, 2, 3, 4, 5]\nshow(a.prefix(2))\nshow(a.suffix(2))\nshow(a.dropFirst())\nshow(a.dropFirst(3))\nshow(a.dropLast())\nshow(a.dropLast(4))\nshow(a.reversed())\nprint(a.prefix(10).count)\nshow(a.prefix(while: { $0 < 3 }))\nshow(a.drop(while: { $0 < 3 }))\nprint([\"a\", \"b\"].reversed().joined(separator: \"\"))\nlet e: [Int] = []\nprint(e.reversed().count)\nprint(a.dropFirst(9).count)"
        ),
        "1,2\n4,5\n2,3,4,5\n4,5\n1,2,3,4\n1\n5,4,3,2,1\n5\n1,2\n3,4,5\nba\n0\n0\n"
    );
}

#[test]
fn a_string_takes_a_prefix_or_drops_while_a_condition_holds() {
    assert_eq!(
        main_printing(
            "let s = \"aab1c\"\nprint(s.prefix(while: { $0 != \"1\" }))\nprint(s.drop(while: { $0 == \"a\" }))"
        ),
        "aab\nb1c\n"
    );
}

#[test]
fn a_negative_count_to_prefix_traps() {
    run_expecting_a_crash("func main() {\n  let a = [1, 2]\n  print(a.prefix(-1).count)\n}");
}

#[test]
fn a_lazy_sequence_evaluates_only_the_elements_that_a_result_needs() {
    assert_eq!(
        main_printing(
            "func show(_ xs: [Int]) {\n  print(xs.map { String($0) }.joined(separator: \",\"))\n}\nvar seen = 0\nlet doubled = [1, 2, 3, 4, 5, 6].lazy.map { (x: Int) -> Int in\n  seen += 1\n  return x * 2\n}\nlet big = doubled.filter { $0 > 4 }\nprint(seen)\nprint(big.first ?? -1)\nprint(seen)\nshow(Array(big.prefix(2)))\nprint(seen)"
        ),
        "0\n6\n3\n6,8\n7\n"
    );
}

#[test]
fn a_lazy_sequence_can_be_restarted_looped_over_and_stopped_early() {
    assert_eq!(
        main_printing(
            "var seen = 0\nlet l = [5, 6, 7, 8].lazy.map { (x: Int) -> Int in\n  seen += 1\n  return x + 1\n}\nfor x in l {\n  if x > 7 { break }\n  print(x)\n}\nprint(seen)\nprint(l.count)\nprint(seen)\nprint(l.isEmpty)\nfor y in l.filter({ $0 % 2 == 0 }) { print(y) }"
        ),
        "6\n7\n3\n4\n7\nfalse\n6\n8\n"
    );
}

#[test]
fn lazy_sequences_offer_the_filtering_and_searching_members() {
    assert_eq!(
        main_printing(
            "func show(_ xs: [Int]) {\n  print(xs.map { String($0) }.joined(separator: \",\"))\n}\nlet a = [1, 2, 3, 4, 5, 6]\nprint(a.lazy.prefix(while: { $0 < 4 }).reduce(0, +))\nshow(Array(a.lazy.drop(while: { $0 < 4 })))\nshow(Array(a.lazy.dropFirst(4)))\nshow(Array(a.lazy.compactMap { (x: Int) -> Int? in\n  if x % 2 == 0 { return x * 10 }\n  return nil\n}))\nprint(a.lazy.first(where: { $0 > 3 }) ?? 0)\nprint(a.lazy.contains(where: { $0 == 9 }))\nprint(a.lazy.allSatisfy { $0 > 0 })\nvar total = 0\na.lazy.filter { $0 > 4 }.forEach { total += $0 }\nprint(total)\nshow(Array(a.lazy.enumerated().map { $0.offset * $0.element }.prefix(3)))\nprint(Set(a.lazy.map { $0 % 3 }).count)"
        ),
        "6\n4,5,6\n5,6\n20,40,60\n4\nfalse\ntrue\n11\n0,2,6\n3\n"
    );
}

#[test]
fn a_lazy_sequence_can_start_from_a_string_a_set_a_dictionary_or_a_range() {
    assert_eq!(
        main_printing(
            "func show(_ xs: [Int]) {\n  print(xs.map { String($0) }.joined(separator: \",\"))\n}\nprint(Array(\"abc\".lazy.map { String($0) + \"!\" }).joined(separator: \"\"))\nlet s: Set<Int> = [4]\nshow(Array(s.lazy.map { $0 + 1 }))\nlet d = [\"k\": 1]\nshow(Array(d.lazy.map { $0.1 }))\nshow(Array((1...4).lazy.filter { $0 % 2 == 1 }))\nshow(Array((0..<3).lazy.map { $0 * $0 }))"
        ),
        "a!b!c!\n5\n1\n1,3\n0,1,4\n"
    );
}

#[test]
fn a_member_named_lazy_still_works_on_a_type_that_declares_it() {
    assert_eq!(
        run(
            "struct Flags {\n  var lazy: Int\n}\nfunc main() {\n  let f = Flags(lazy: 3)\n  print(f.lazy)\n}"
        ),
        "3\n"
    );
}

#[test]
fn the_library_members_reject_wrong_arguments() {
    for (body, message) in [
        (
            "let a = [1]\n  let b = a.prefix()",
            "takes 1 argument(s), but 0 were given",
        ),
        ("let b = 5.lazy", "has no members"),
        (
            "let a = [1]\n  let b = a.reversed(1)",
            "takes 0 argument(s), but 1 were given",
        ),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        assert!(build_error(&source).contains(message), "{body}");
    }
}

#[test]
fn a_ternary_picks_one_of_two_values_and_evaluates_only_that_one() {
    assert_eq!(
        main_printing(
            "var calls = 0\nfunc side(_ n: Int) -> Int {\n  calls += 1\n  return n\n}\nlet a = 5\nprint(a > 3 ? side(1) : side(2))\nprint(a > 9 ? side(1) : side(2))\nprint(calls)\nprint(a > 3 ? \"big\" : \"small\")\nlet nested = a < 3 ? 0 : a < 10 ? 1 : 2\nprint(nested)\nprint(1 + (a > 3 ? 10 : 20) * 2)\nprint(a > 3 && a < 9 ? 1 : 0)\nlet flag: Bool = a == 5 ? true : false\nprint(flag)"
        ),
        "1\n2\n2\nbig\n1\n21\n1\ntrue\n"
    );
}

#[test]
fn a_ternary_takes_optionals_enum_cases_and_values_of_any_kind() {
    assert_eq!(
        run(
            "enum Color {\n  case red\n  case green\n}\nclass Box {\n  var n: Int\n}\nstruct P {\n  var x: Int\n}\nfunc main() {\n  let t = true\n  let o: Int? = t ? 4 : nil\n  print(o ?? 0)\n  let p: Int? = t ? nil : 4\n  print(p ?? -1)\n  let c: Color = t ? .green : .red\n  print(c == .green)\n  let b = t ? Box(n: 1) : Box(n: 2)\n  print(b.n)\n  let s = t ? P(x: 3) : P(x: 4)\n  print(s.x)\n  let words = t ? [\"a\", \"b\"] : []\n  print(words.count)\n  let m = t ? \"x\" + \"y\" : \"z\"\n  print(m)\n  let q = t ? o : nil\n  print(q ?? 0)\n  let f = t ? { (x: Int) -> Int in x + 1 } : { (x: Int) -> Int in x }\n  print(f(1))\n}"
        ),
        "4\n-1\ntrue\n1\n3\n2\nxy\n4\n2\n"
    );
}

#[test]
fn a_ternary_may_span_lines_and_sit_in_arguments_and_returns() {
    assert_eq!(
        main_printing(
            "func pick(_ a: Int, _ b: Int) -> Int {\n  return a > b ? a\n    : b\n}\nprint(pick(3, 8))\nlet xs = [1, 2, 3]\nprint(xs.map { $0 % 2 == 0 ? $0 * 10 : $0 }.reduce(0, +))\nprint(pick(xs.count > 2 ? 1 : 2, 0))\nlet maybe: Int? = nil\nprint(maybe ?? 0)\nprint(maybe == nil ? \"empty\" : \"full\")"
        ),
        "8\n24\n1\n0\nempty\n"
    );
}

#[test]
fn a_ternary_is_checked_for_its_condition_and_branch_types() {
    for (body, message) in [
        ("let x = 1 ? 2 : 3", "expected Bool"),
        ("let x = true ? 1 : \"a\"", "expected"),
        ("let x = true ? nil : nil", "nil"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        assert!(build_error(&source).contains(message), "{body}");
    }
}

#[test]
fn a_ternary_releases_what_it_creates_and_keeps_what_it_borrows() {
    assert_eq!(
        run(
            "class Tracked {\n  var n: Int\n  deinit { print(\"free \\(n)\") }\n}\nfunc make(_ c: Bool, _ k: Tracked) -> Tracked {\n  return c ? k : Tracked(n: 9)\n}\nfunc label(_ c: Bool) -> String {\n  let s = \"kept\"\n  return c ? s : \"x\" + \"y\"\n}\nfunc inner() {\n  let k = Tracked(n: 1)\n  let a = make(true, k)\n  let b = make(false, k)\n  print(a.n + b.n)\n  var total = 0\n  var i = 0\n  while i < 3 {\n    let t = i % 2 == 0 ? Tracked(n: 100 + i) : k\n    total += t.n\n    i += 1\n  }\n  print(total)\n  print(label(true) + label(false))\n}\nfunc main() {\n  inner()\n  print(\"done\")\n}"
        ),
        "10\nfree 100\nfree 102\n203\nkeptxy\nfree 9\nfree 1\ndone\n"
    );
}

#[test]
fn top_level_statements_run_as_the_program_without_a_main_function() {
    assert_eq!(
        run(
            "import Swift\nprint(1)\nlet a = 2\nprint(a + 1)\nfunc f() -> Int {\n  return 10\n}\nprint(f())\nstruct P {\n  var x: Int\n}\nprint(P(x: 4).x)"
        ),
        "1\n3\n10\n4\n"
    );
}

#[test]
fn a_function_reads_and_writes_a_top_level_variable() {
    assert_eq!(
        run(
            "var counter = 0\nfunc bump() {\n  counter += 1\n}\nbump()\nbump()\nprint(counter)\nlet label = \"n\"\nfunc show() {\n  print(label + String(counter))\n}\nshow()\ncounter = 10\nshow()"
        ),
        "2\nn2\nn10\n"
    );
}

#[test]
fn top_level_variables_of_every_kind_can_be_changed_from_a_function() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n}\nclass Box {\n  var n: Int\n}\nvar p = P(x: 1)\nvar xs = [1, 2]\nvar d: [String: Int] = [:]\nvar b = Box(n: 5)\nvar o: Int? = nil\nvar s = \"a\"\nfunc touch() {\n  p.x += 1\n  xs.append(3)\n  xs[0] = 9\n  d[\"k\"] = 7\n  b.n += 1\n  o = 9\n  s += \"b\"\n}\ntouch()\nprint(p.x)\nprint(xs.count + xs[0])\nprint(d[\"k\"] ?? 0)\nprint(b.n)\nprint(o ?? 0)\nprint(s)"
        ),
        "2\n12\n7\n6\n9\nab\n"
    );
}

#[test]
fn closures_and_methods_share_top_level_variables() {
    assert_eq!(
        run(
            "var total = 0\nlet add = { (x: Int) -> Void in\n  total += x\n}\nadd(2)\nadd(3)\nprint(total)\nfunc viaFunc() {\n  [1, 2].forEach { total += $0 }\n}\nviaFunc()\nprint(total)\nstruct S {\n  var v: Int\n  func hit() {\n    total += v\n  }\n}\nS(v: 4).hit()\nprint(total)\nlet shadow = 1\nfunc keep(_ shadow: Int) -> Int {\n  return shadow + 100\n}\nprint(keep(5) + shadow)"
        ),
        "5\n8\n12\n106\n"
    );
}

#[test]
fn top_level_control_flow_and_objects_are_released() {
    assert_eq!(
        run(
            "class T {\n  var n: Int\n  deinit {\n    print(\"free \\(n)\")\n  }\n}\nvar sum = 0\nfor i in 0..<4 {\n  sum += i\n}\nif sum > 3 {\n  print(\"big\")\n} else {\n  print(\"small\")\n}\nwhile sum < 10 {\n  sum += 1\n}\nprint(sum)\nvar t = T(n: 1)\nt = T(n: 2)\nprint(\"end\")"
        ),
        "big\n10\nfree 1\nend\n"
    );
}

#[test]
fn a_program_with_top_level_statements_is_checked_for_main_and_declarations() {
    for (source, message) in [
        ("let a = 1\nlet a = 2", "already declared"),
        ("print(late)\nlet late = 1", "cannot find 'late'"),
        ("let c = 1\nc = 2", "constant"),
    ] {
        assert!(build_error(source).contains(message), "{source}");
    }
}

#[test]
fn a_program_without_top_level_statements_still_needs_main() {
    assert!(build_error("func other() {\n}").contains("main"));
}

#[test]
fn a_function_named_main_is_an_ordinary_function_next_to_top_level_statements() {
    assert_eq!(
        run("func main() {\n  print(\"helper\")\n}\nprint(\"top\")\nmain()\nprint(\"end\")"),
        "top\nhelper\nend\n"
    );
}

#[test]
fn void_may_be_written_as_the_result_type_of_any_function() {
    assert_eq!(
        run(
            "protocol Greeter {\n  func greet() -> Void\n}\nstruct S: Greeter {\n  func greet() -> Void {\n    print(3)\n  }\n}\nfunc f() -> Void {\n  print(1)\n}\nfunc g(_ h: () -> Void) -> Void {\n  h()\n}\nf()\ng({ print(2) })\nS().greet()"
        ),
        "1\n2\n3\n"
    );
}

#[test]
fn string_escapes_include_unicode_scalars_and_integer_literals_have_other_bases() {
    assert_eq!(
        main_printing(
            "print(\"a\\u{41}\\u{e9}b\")\nprint(\"x\\ty\\rz\".count)\nprint(0xFF + 0b101 + 0o17 + 1_000)\nprint(0x7fff_ffff_ffff_ffff == 9223372036854775807)"
        ),
        "aAéb\n5\n1275\ntrue\n"
    );
}

#[test]
fn a_parameter_may_have_no_name() {
    assert_eq!(
        run(
            "protocol Sink {\n  func take(_: Int) -> Int\n}\nstruct S: Sink {\n  func take(_: Int) -> Int {\n    return 7\n  }\n}\nfunc ignore(_: Int, _ b: Int, _: String) -> Int {\n  return b\n}\nprint(ignore(1, 2, \"x\") + S().take(9))"
        ),
        "9\n"
    );
}

#[test]
fn access_modifiers_and_inline_attributes_are_accepted_and_ignored() {
    assert_eq!(
        run(
            "public struct S {\n  private(set) var x: Int\n  public func f() -> Int {\n    return x\n  }\n}\n@inline(never)\nfunc g() -> Int {\n  return 2\n}\npublic final class C {\n  internal var n: Int\n  fileprivate func h() -> Int {\n    return n\n  }\n}\n@_eagerMove class D {\n  var m: Int\n}\nfunc take(_ f: @escaping () -> Int) -> Int {\n  return f()\n}\nprint(S(x: 1).f() + g() + C(n: 4).h() + D(m: 8).m + take({ 16 }))"
        ),
        "31\n"
    );
}

#[test]
fn several_names_may_be_declared_together() {
    assert_eq!(
        run(
            "struct P {\n  var x, y: Int\n  let tag: String\n}\nclass C {\n  var a, b, c: Int\n}\nlet one = 1, two = 2\nvar p = P(x: 3, y: 4, tag: \"t\")\nfunc f() -> Int {\n  var s = 0, t = 10\n  s += 5\n  return s + t\n}\nprint(one + two + p.x + p.y + C(a: 1, b: 2, c: 3).c + f())"
        ),
        "28\n"
    );
}

#[test]
fn a_stored_property_may_have_a_default_value() {
    assert_eq!(
        run(
            "struct S {\n  var a = 1\n  var b: Int\n  var tag = \"t\"\n  var on = true\n  var xs: [Int] = []\n}\nclass Node {\n  var value = 7\n  var next: Node? = nil\n}\nlet s = S(b: 2)\nlet t = S(a: 5, b: 6, tag: \"u\", on: false, xs: [1])\nlet n = Node()\nlet m = Node(value: 9, next: n)\nprint(s.a + s.b + t.a + t.b)\nprint(s.tag + t.tag)\nprint(s.on)\nprint(t.on)\nprint(s.xs.count + t.xs.count)\nprint(n.value + (m.next?.value ?? 0) + m.value)\nprint(n.next == nil)"
        ),
        "14\ntu\ntrue\nfalse\n1\n23\ntrue\n"
    );
}

#[test]
fn an_initializer_starts_from_the_default_values_of_its_type() {
    assert_eq!(
        run(
            "class C {\n  var a = 3\n  var b: Int\n  var c = \"x\"\n  init(b: Int) {\n    self.b = b\n  }\n  init(a: Int, b: Int) {\n    self.a = a\n    self.b = b\n    self.c = \"y\"\n  }\n  convenience init() {\n    self.init(b: 40)\n  }\n}\nstruct P {\n  var x = 1\n  var y: Int\n  init(y: Int) {\n    self.y = y\n  }\n}\nlet c = C(b: 4)\nlet d = C(a: 1, b: 2)\nprint(c.a + c.b)\nprint(c.c + d.c)\nprint(d.a + d.b)\nprint(C().b)\nprint(P(y: 5).x + P(y: 5).y)"
        ),
        "7\nxy\n3\n40\n6\n"
    );
}

#[test]
fn a_member_default_that_is_not_a_literal_needs_a_type_and_labels_stay_in_order() {
    for (source, message) in [
        (
            "struct S {\n  var a = [1]\n}\nfunc main() {\n}",
            "give the member 'a' a type",
        ),
        (
            "struct S {\n  var a = 1\n  var b: Int\n}\nfunc main() {\n  let s = S(b: 1, a: 2)\n}",
            "in the order",
        ),
        (
            "struct S {\n  var a = 1\n  var b: Int\n}\nfunc main() {\n  let s = S()\n}",
            "'b'",
        ),
        (
            "struct S {\n  var a, b = 1\n}\nfunc main() {\n}",
            "one name",
        ),
    ] {
        assert!(build_error(source).contains(message), "{source}");
    }
}

#[test]
fn print_takes_several_items_a_separator_and_a_terminator() {
    assert_eq!(
        main_printing(
            "print(1, 2, 3)\nprint(\"a\", \"b\", separator: \"-\")\nprint(\"x\", terminator: \"\")\nprint(\"y\", terminator: \"!\\n\")\nprint(1, true, \"s\", separator: \", \", terminator: \";\\n\")\nprint()\nprint(\"\", terminator: \"\")\nprint(\"end\")\nlet c: Character = \"c\"\nprint(c, (1, 2), terminator: \"\\n\")\nprint(7, terminator: \"|\")\nprint(8)"
        ),
        "1 2 3\na-b\nxy!\n1, true, s;\n\nend\nc (1, 2)\n7|8\n"
    );
}

#[test]
fn print_arguments_are_checked() {
    for (body, message) in [
        ("print(1, terminator: 2)", "expected String"),
        ("print(1, sep: \"-\")", "print takes"),
        (
            "print(1, separator: \"a\", separator: \"b\")",
            "print takes",
        ),
        ("print(terminator: \"x\", 1)", "print takes"),
        ("print([1])", "cannot print"),
    ] {
        let source = format!("func main() {{\n  {body}\n}}");
        assert!(build_error(&source).contains(message), "{body}");
    }
}

#[test]
fn an_inout_parameter_changes_the_variable_that_the_caller_passes() {
    assert_eq!(
        run(
            "func bump(_ x: inout Int) {\n  x += 1\n}\nfunc twice(_ x: inout Int) {\n  bump(&x)\n  bump(&x)\n}\nfunc swapTwo<T>(_ a: inout T, _ b: inout T) {\n  let t = a\n  a = b\n  b = t\n}\nfunc grow(_ xs: inout [Int], by n: Int) -> Int {\n  xs.append(n)\n  return xs.count\n}\nvar n = 1\nbump(&n)\ntwice(&n)\nprint(n)\nvar a = \"left\"\nvar b = \"right\"\nswapTwo(&a, &b)\nprint(a + b)\nvar x = 1\nvar y = 2\nswapTwo(&x, &y)\nprint(x * 10 + y)\nvar xs = [1]\nlet count = grow(&xs, by: 5)\nprint(count + xs[1])"
        ),
        "4\nrightleft\n21\n7\n"
    );
}

#[test]
fn an_inout_argument_may_be_a_member_an_element_or_a_global() {
    assert_eq!(
        run(
            "struct P {\n  var x: Int\n  var names: [String]\n}\nclass Box {\n  var n: Int\n}\nvar total = 0\nfunc add(_ x: inout Int, _ by: Int) {\n  x += by\n}\nfunc rename(_ s: inout String) {\n  s += \"!\"\n}\nvar p = P(x: 1, names: [\"a\"])\nvar xs = [10, 20]\nlet b = Box(n: 5)\nadd(&p.x, 2)\nadd(&xs[1], 3)\nadd(&b.n, 4)\nadd(&total, 7)\nrename(&p.names[0])\nprint(p.x)\nprint(xs[1])\nprint(b.n)\nprint(total)\nprint(p.names[0])"
        ),
        "3\n23\n9\n7\na!\n"
    );
}

#[test]
fn an_inout_method_parameter_and_a_closure_that_uses_it_work() {
    assert_eq!(
        run(
            "struct Acc {\n  var base: Int\n  func fold(into total: inout Int) {\n    [1, 2, 3].forEach { total += $0 * base }\n  }\n}\nvar t = 100\nAcc(base: 2).fold(into: &t)\nprint(t)"
        ),
        "112\n"
    );
}

#[test]
fn an_inout_parameter_releases_and_keeps_objects_correctly() {
    assert_eq!(
        run(
            "class T {\n  var n: Int\n  deinit {\n    print(\"free \\(n)\")\n  }\n}\nfunc replace(_ t: inout T, with n: Int) {\n  t = T(n: n)\n}\nfunc inner() {\n  var t = T(n: 1)\n  replace(&t, with: 2)\n  print(\"after \\(t.n)\")\n  replace(&t, with: 3)\n}\ninner()\nprint(\"end\")"
        ),
        "free 1\nafter 2\nfree 2\nfree 3\nend\n"
    );
}

#[test]
fn inout_arguments_and_parameters_are_checked() {
    for (source, message) in [
        (
            "func f(_ x: inout Int) {\n}\nfunc main() {\n  var a = 1\n  f(a)\n}",
            "needs '&'",
        ),
        (
            "func f(_ x: Int) {\n}\nfunc main() {\n  var a = 1\n  f(&a)\n}",
            "not an inout parameter",
        ),
        (
            "func f(_ x: inout Int) {\n}\nfunc main() {\n  let a = 1\n  f(&a)\n}",
            "constant",
        ),
        (
            "func f(_ x: inout Int) {\n}\nfunc main() {\n  f(&1)\n}",
            "can be changed",
        ),
        (
            "func f(_ x: inout Int) {\n}\nfunc main() {\n  var s = \"a\"\n  f(&s)\n}",
            "expected Int",
        ),
        (
            "func main() {\n  var a = 1\n  let b = &a\n}",
            "inout parameter",
        ),
        (
            "func main() {\n  func g(_ x: inout Int) {\n  }\n}",
            "local function",
        ),
    ] {
        assert!(build_error(source).contains(message), "{source}");
    }
}

#[test]
fn a_protocol_may_inherit_from_other_protocols() {
    assert_eq!(
        run(
            "protocol Named {\n  func name() -> String\n}\nprotocol Aged {\n  func age() -> Int\n}\nprotocol Person: Named, Aged {\n  func greet() -> String\n}\nstruct Ann: Person {\n  func name() -> String {\n    return \"ann\"\n  }\n  func age() -> Int {\n    return 30\n  }\n  func greet() -> String {\n    return \"hi \" + name()\n  }\n}\nfunc describe<T: Person>(_ p: T) -> String {\n  return p.name() + String(p.age()) + p.greet()\n}\nfunc onlyName<T: Named>(_ p: T) -> String {\n  return p.name()\n}\nfunc viaPerson<T: Person>(_ p: T) -> String {\n  return onlyName(p)\n}\nprint(describe(Ann()))\nprint(viaPerson(Ann()))\nprint(onlyName(Ann()))"
        ),
        "ann30hi ann\nann\nann\n"
    );
}

#[test]
fn a_type_that_conforms_to_a_protocol_must_implement_what_it_inherits() {
    let source = "protocol A {\n  func a() -> Int\n}\nprotocol B: A {\n  func b() -> Int\n}\nstruct S: B {\n  func b() -> Int {\n    return 1\n  }\n}\nfunc main() {\n}";
    assert!(build_error(source).contains("'a'"));
}

#[test]
fn inheritance_through_several_levels_and_a_diamond_is_followed() {
    assert_eq!(
        run(
            "protocol Base {\n  func base() -> Int\n}\nprotocol Left: Base {\n  func left() -> Int\n}\nprotocol Right: Base {\n  func right() -> Int\n}\nprotocol Both: Left, Right {\n}\nstruct S: Both {\n  func base() -> Int {\n    return 1\n  }\n  func left() -> Int {\n    return 10\n  }\n  func right() -> Int {\n    return 100\n  }\n}\nfunc total<T: Base>(_ t: T) -> Int {\n  return t.base()\n}\nfunc all<T: Both>(_ t: T) -> Int {\n  return t.base() + t.left() + t.right() + total(t)\n}\nprint(all(S()))"
        ),
        "112\n"
    );
}

#[test]
fn a_class_only_protocol_is_accepted_for_classes_only() {
    assert_eq!(
        run(
            "protocol Counter: class {\n  func bump() -> Int\n}\nprotocol Anyobj: AnyObject {\n  func tag() -> Int\n}\nclass C: Counter, Anyobj {\n  var n: Int\n  func bump() -> Int {\n    n += 1\n    return n\n  }\n  func tag() -> Int {\n    return 7\n  }\n}\nfunc use<T: Counter>(_ c: T) -> Int {\n  return c.bump() + c.bump()\n}\nprint(use(C(n: 0)) + C(n: 0).tag())"
        ),
        "10\n"
    );
    let source = "protocol Counter: class {\n  func bump() -> Int\n}\nstruct S: Counter {\n  func bump() -> Int {\n    return 1\n  }\n}\nfunc main() {\n}";
    assert!(build_error(source).contains("class"));
}

#[test]
fn protocol_inheritance_is_checked_for_unknown_names_and_cycles() {
    for (source, message) in [
        ("protocol A: Missing {\n}\nfunc main() {\n}", "Missing"),
        (
            "protocol A: B {\n}\nprotocol B: A {\n}\nfunc main() {\n}",
            "cycle",
        ),
        ("protocol A: A {\n}\nfunc main() {\n}", "cycle"),
    ] {
        assert!(build_error(source).contains(message), "{source}");
    }
}

#[test]
fn a_where_clause_adds_conformance_requirements_to_type_parameters() {
    assert_eq!(
        run(
            "protocol Named {\n  func name() -> String\n}\nprotocol Aged {\n  func age() -> Int\n}\nstruct Ann: Named, Aged {\n  func name() -> String {\n    return \"ann\"\n  }\n  func age() -> Int {\n    return 3\n  }\n}\nfunc show<T, U>(_ a: T, _ b: U) -> String where T: Named, U: Named & Aged {\n  return a.name() + b.name() + String(b.age())\n}\nstruct Pair<A, B> where A: Named, B: Aged {\n  var a: A\n  var b: B\n  func both() -> String {\n    return a.name() + String(b.age())\n  }\n}\nprint(show(Ann(), Ann()))\nprint(Pair(a: Ann(), b: Ann()).both())"
        ),
        "annann3\nann3\n"
    );
}

#[test]
fn where_clauses_are_checked() {
    for (source, message) in [
        (
            "protocol P {\n}\nfunc f<T>(_ t: T) where T: P {\n}\nstruct S {\n}\nfunc main() {\n  f(S())\n}",
            "does not conform",
        ),
        (
            "func f<T>(_ t: T) where U: Equatable {\n}\nfunc main() {\n}",
            "'U'",
        ),
        (
            "func f<T, U>(_ t: T, _ u: U) where T == U {\n}\nfunc main() {\n}",
            "same-type",
        ),
        (
            "func f<T: Sequence>(_ t: T) where T.Element: Equatable {\n}\nfunc main() {\n}",
            "not supported",
        ),
    ] {
        assert!(build_error(source).contains(message), "{source}");
    }
}

#[test]
fn a_value_of_an_inheriting_protocol_calls_the_requirements_it_inherits() {
    assert_eq!(
        run(
            "protocol Named {\n  func name() -> String\n}\nprotocol Person: Named {\n  func age() -> Int\n}\nprotocol Source {\n  associatedtype Item\n  func first() -> Item\n}\nprotocol Eq2: Equatable {\n  func tag() -> Int\n}\nstruct Ann: Person, Source, Eq2 {\n  typealias Item = Int\n  var x: Int\n  func name() -> String {\n    return \"ann\"\n  }\n  func age() -> Int {\n    return 3\n  }\n  func first() -> Int {\n    return 9\n  }\n  func tag() -> Int {\n    return x\n  }\n}\nstruct Bob: Named {\n  func name() -> String {\n    return \"bob\"\n  }\n}\nfunc same<T: Eq2>(_ a: T, _ b: T) -> Bool {\n  return a == b\n}\nfunc item<T: Source>(_ t: T) -> T.Item {\n  return t.first()\n}\nlet p: Person = Ann(x: 1)\nlet n: Named = Bob()\nprint(p.name() + String(p.age()) + n.name())\nprint(same(Ann(x: 1), Ann(x: 1)))\nprint(item(Ann(x: 1)))"
        ),
        "ann3bob\ntrue\n9\n"
    );
}

#[test]
fn an_extension_adds_methods_and_initializers_to_a_type_declared_anywhere() {
    assert_eq!(
        run(
            "extension Point {\n  func sum() -> Int {\n    return x + y\n  }\n  mutating func shift(by n: Int) {\n    x += n\n  }\n  init(both n: Int) {\n    self.x = n\n    self.y = n\n  }\n}\nextension Counter {\n  func bump() -> Int {\n    n += 1\n    return n\n  }\n}\nextension Color {\n  func code() -> Int {\n    switch self {\n    case .red: return 1\n    case .green: return 2\n    }\n  }\n}\nstruct Point {\n  var x: Int\n  var y: Int\n}\nclass Counter {\n  var n: Int\n}\nenum Color {\n  case red\n  case green\n}\nvar p = Point(both: 3)\np.shift(by: 4)\nprint(p.sum())\nlet c = Counter(n: 0)\nprint(c.bump() + c.bump())\nprint(Color.green.code())"
        ),
        "10\n3\n2\n"
    );
}

#[test]
fn an_extension_declares_a_conformance_and_binds_an_associated_type() {
    assert_eq!(
        run(
            "protocol Source {\n  associatedtype Item\n  func first() -> Item\n}\nprotocol Named {\n  func name() -> String\n}\nstruct S {\n  var v: Int\n}\nextension S: Named {\n  func name() -> String {\n    return \"s\" + String(v)\n  }\n}\nextension S: Source {\n  typealias Item = Int\n  func first() -> Int {\n    return v\n  }\n}\nfunc show<T: Named>(_ t: T) -> String {\n  return t.name()\n}\nfunc item<T: Source>(_ t: T) -> T.Item {\n  return t.first()\n}\nlet boxed: Named = S(v: 2)\nprint(show(S(v: 1)) + boxed.name())\nprint(item(S(v: 8)))"
        ),
        "s1s2\n8\n"
    );
}

#[test]
fn an_extension_may_add_methods_and_conformances_to_the_basic_types() {
    assert_eq!(
        run(
            "protocol Describable {\n  func describe() -> String\n}\nextension Int: Describable {\n  func describe() -> String {\n    return \"int \" + String(self)\n  }\n  func doubled() -> Int {\n    return self * 2\n  }\n}\nextension String: Describable {\n  func describe() -> String {\n    return \"string \" + self\n  }\n}\nextension Bool {\n  func flipped() -> Bool {\n    return !self\n  }\n}\nfunc show<T: Describable>(_ t: T) -> String {\n  return t.describe()\n}\nprint(5.describe())\nprint(show(7))\nprint(show(\"x\"))\nprint(21.doubled())\nprint(true.flipped())\nlet n = 4\nprint(n.doubled().describe())"
        ),
        "int 5\nint 7\nstring x\n42\nfalse\nint 8\n"
    );
}

#[test]
fn an_extension_of_a_generic_type_sees_its_type_parameters() {
    assert_eq!(
        run(
            "extension Box {\n  func twice() -> [T] {\n    return [value, value]\n  }\n  func map<U>(_ f: (T) -> U) -> Box<U> {\n    return Box<U>(value: f(value))\n  }\n}\nstruct Box<T> {\n  var value: T\n}\nprint(Box(value: 3).twice().count)\nprint(Box(value: 4).map { $0 + 1 }.value)\nprint(Box(value: \"s\").twice()[1])"
        ),
        "2\n5\ns\n"
    );
}

#[test]
fn extensions_are_checked() {
    for (source, message) in [
        (
            "extension Missing {\n  func f() { }\n}\nfunc main() {\n}",
            "unknown type 'Missing'",
        ),
        (
            "protocol P {\n}\nextension P {\n  func f() { }\n}\nfunc main() {\n}",
            "extension of a protocol",
        ),
        (
            "struct S {\n}\nextension S where S: Equatable {\n}\nfunc main() {\n}",
            "where",
        ),
        (
            "struct S {\n  func f() { }\n}\nextension S {\n  func f() { }\n}\nfunc main() {\n}",
            "f",
        ),
        (
            "struct S {\n}\nextension S {\n  var x: Int\n}\nfunc main() {\n}",
            "stored propert",
        ),
        (
            "extension Int {\n  mutating func up() { }\n}\nfunc main() {\n}",
            "mutating",
        ),
        (
            "extension Array {\n  func f() { }\n}\nfunc main() {\n}",
            "cannot be extended",
        ),
        (
            "protocol P {\n  func f()\n}\nextension Int: P {\n}\nfunc main() {\n}",
            "'f'",
        ),
    ] {
        assert!(build_error(source).contains(message), "{source}");
    }
}

#[test]
fn a_ternary_may_start_its_branches_on_new_lines() {
    assert_eq!(
        main_printing(
            "let n = 15\nlet word = n % 15 == 0\n  ? \"fizzbuzz\"\n  : n % 5 == 0\n    ? \"buzz\"\n    : \"other\"\nprint(word)\nlet pick = n > 3 ? 1\n  : 2\nprint(pick)"
        ),
        "fizzbuzz\n1\n"
    );
}

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import swift_tests as st


def classify(text):
    return st.classify_text(text)


class FileCheckTests(unittest.TestCase):
    def check(self, source, output):
        return st.filecheck(st.check_directives(source), output)

    def test_a_check_matches_a_later_line(self):
        self.assertTrue(self.check("// CHECK: 3", "1\n2\n3\n"))

    def test_checks_must_match_in_order(self):
        self.assertFalse(self.check("// CHECK: 3\n// CHECK: 1", "1\n2\n3\n"))

    def test_check_next_needs_the_following_line(self):
        source = "// CHECK: 1\n// CHECK-NEXT: 2"
        self.assertTrue(self.check(source, "1\n2\n"))
        self.assertFalse(self.check(source, "1\nx\n2\n"))

    def test_check_same_needs_the_same_line(self):
        source = "// CHECK: a\n// CHECK-SAME: c"
        self.assertTrue(self.check(source, "a b c\n"))
        self.assertFalse(self.check(source, "a b\nc\n"))

    def test_check_not_forbids_text_before_the_next_match(self):
        source = "// CHECK: 1\n// CHECK-NOT: x\n// CHECK: 3"
        self.assertTrue(self.check(source, "1\n2\n3\n"))
        self.assertFalse(self.check(source, "1\nx\n3\n"))

    def test_a_trailing_check_not_covers_the_rest_of_the_output(self):
        source = "// CHECK: 1\n// CHECK-NOT: x"
        self.assertFalse(self.check(source, "1\nx\n"))
        self.assertTrue(self.check(source, "1\ny\n"))

    def test_regex_parts_and_whitespace_are_flexible(self):
        self.assertTrue(self.check("// CHECK: a {{[0-9]+}} b", "a   42\tb\n"))

    def test_text_is_matched_literally(self):
        self.assertFalse(self.check("// CHECK: a.c", "abc\n"))
        self.assertTrue(self.check("// CHECK: a.c", "a.c\n"))

    def test_a_test_without_checks_passes_on_any_output(self):
        self.assertTrue(self.check("print(1)", "whatever"))

    def test_a_label_is_matched_like_a_check(self):
        self.assertTrue(self.check("// CHECK-LABEL: start", "x\nstart\n"))


class ClassifyTests(unittest.TestCase):
    def applicable(self, text):
        return classify(text)[1] is None

    def test_the_simple_run_form_is_applicable(self):
        text = "// RUN: %target-run-simple-swift | %FileCheck %s\n// REQUIRES: executable_test\nprint(1)\n"
        self.assertEqual(classify(text), ("run", None))

    def test_the_build_then_run_form_is_applicable(self):
        text = (
            "// RUN: %empty-directory(%t)\n"
            "// RUN: %target-build-swift %s -o %t/a.out\n"
            "// RUN: %target-codesign %t/a.out\n"
            "// RUN: %target-run %t/a.out | %FileCheck %s\n"
        )
        self.assertTrue(self.applicable(text))

    def test_run_lines_with_flags_are_not_applicable(self):
        text = "// RUN: %target-run-simple-swift(-O) | %FileCheck %s\n"
        self.assertEqual(classify(text)[1], "compile flags")
        text = "// RUN: %target-build-swift -O %s -o %t/a.out\n// RUN: %target-run %t/a.out\n"
        self.assertEqual(classify(text)[1], "compile flags")

    def test_other_run_lines_are_not_applicable(self):
        self.assertEqual(classify("// RUN: %swift -frontend %s\n")[1], "other RUN line")
        self.assertEqual(classify("print(1)\n")[1], "no RUN line")

    def test_filecheck_options_are_not_applicable(self):
        text = "// RUN: %target-run-simple-swift | %FileCheck %s --check-prefix=A\n"
        self.assertEqual(classify(text)[1], "FileCheck options")

    def test_requirements_that_exclude_this_platform_are_not_applicable(self):
        base = "// RUN: %target-run-simple-swift\n"
        self.assertEqual(classify(base + "// REQUIRES: objc_interop\n")[1], "requires objc_interop")
        self.assertIsNone(classify(base + "// REQUIRES: executable_test, OS=macosx\n")[1])
        self.assertEqual(classify(base + "// UNSUPPORTED: OS=macosx\n")[1], "unsupported on macosx")
        self.assertEqual(classify(base + "// XFAIL: *\n")[1], "XFAIL")

    def test_imports_other_than_swift_are_not_applicable(self):
        base = "// RUN: %target-run-simple-swift\n"
        self.assertEqual(classify(base + "import Foundation\n")[1], "imports Foundation")
        self.assertIsNone(classify(base + "import Swift\n")[1])

    def test_unsupported_features_are_reported_by_name(self):
        base = "// RUN: %target-run-simple-swift\n"
        self.assertEqual(classify(base + "let x: Double = 1\n")[1], "numeric types")
        self.assertEqual(classify(base + "func f() async {}\n")[1], "concurrency")
        self.assertEqual(classify(base + "#if os(macOS)\n#endif\n")[1], "#if")
        self.assertEqual(classify(base + "@available(macOS 11, *) func f() {}\n")[1], "@available")

    def test_words_in_comments_do_not_count(self):
        base = "// RUN: %target-run-simple-swift\n"
        self.assertIsNone(classify(base + "// Double is not used here\nprint(1)\n")[1])

    def test_unsupported_filecheck_syntax_is_not_applicable(self):
        base = "// RUN: %target-run-simple-swift | %FileCheck %s\n"
        self.assertEqual(classify(base + "// CHECK-DAG: a\n")[1], "FileCheck directive CHECK-DAG")
        self.assertEqual(classify(base + "// CHECK: [[X:a+]]\n")[1], "FileCheck variables")


class FirstErrorTests(unittest.TestCase):
    def test_the_first_error_is_normalized_into_a_category(self):
        stderr = "/tmp/x/a.swift:12: error: cannot find 'foo' in scope\n"
        self.assertEqual(st.error_category(stderr), "cannot find '_' in scope")

    def test_numbers_are_normalized(self):
        stderr = "a.swift:3: error: 'f' takes 2 argument(s), but 3 were given\n"
        self.assertEqual(st.error_category(stderr), "'_' takes N argument(s), but N were given")

    def test_no_error_line_gives_a_placeholder(self):
        self.assertEqual(st.error_category("boom"), "(no error message)")


if __name__ == "__main__":
    unittest.main()

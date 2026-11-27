#!/usr/bin/env python3
"""Run apus against the run-pass tests of swiftlang/swift (`test/Interpreter`) and report how much of it passes.

usage: python3 bench/swift_tests.py --swift-repo PATH [--apus target/release/apus] [--filter SUBSTR] [--limit N]
                                    [--jobs N] [--json FILE] [--results FILE] [--top N]
                                    [--baseline FILE [--check | --write-baseline]]
PATH is a checkout of swiftlang/swift at the pinned commit (or set APUS_SWIFT_REPO). Run from the repository root
after `cargo build --release`.

A test is applicable when its RUN lines are the plain forms (`%target-run-simple-swift`, or `%target-build-swift %s
-o %t/x` followed by `%target-run %t/x`, optionally piped into `%FileCheck %s`), its REQUIRES / UNSUPPORTED / XFAIL
lines do not exclude this platform, it imports nothing but Swift, and it uses none of the features the dialect cannot
express (numeric types other than Int, concurrency, `#if`, `@available`, noncopyable types, macros, unsafe pointers).
Every other test is counted as not applicable, with its reason, and is never a failure.

A test passes when apus builds it, the program exits with 0, and its standard output satisfies the CHECK lines
(CHECK, CHECK-NEXT, CHECK-SAME, CHECK-NOT, CHECK-LABEL; literal text and {{regex}} parts).
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COMMIT = "fb5b4b0a959e61020193cc0bad376d65279385d9"
SUITE = "test/Interpreter"
OK_REQUIREMENTS = {"executable_test", "OS=macosx", "CPU=arm64"}
PLATFORM_WORDS = ("macosx", "macos", "arm64", "*")
CHECK_KINDS = {"CHECK", "CHECK-NEXT", "CHECK-SAME", "CHECK-NOT", "CHECK-LABEL"}
FEATURES = [
    ("numeric types", r"\b(Double|Float|Float16|Float80|CGFloat|U?Int(8|16|32|64)|UInt)\b"),
    ("concurrency", r"\b(async|await|actor|Task|MainActor|Sendable)\b"),
    ("#if", r"^\s*#if\b"),
    ("@available", r"@available\b"),
    ("noncopyable types", r"~Copyable|\bborrowing\b|\bconsuming\b|\b_?move\b"),
    ("macros", r"#externalMacro|@freestanding|@attached|\bmacro\b"),
    ("unsafe pointers", r"\bUnsafe\w*|\bwithUnsafe\w*"),
]


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//.*", "", text)


def run_lines(text):
    lines, pending = [], None
    for raw in text.split("\n"):
        m = re.match(r"\s*//\s*RUN:\s?(.*)$", raw)
        if pending is not None:
            m2 = re.match(r"\s*//\s?(.*)$", raw)
            piece = m2.group(1) if m2 else raw
            pending += " " + piece.strip()
        elif m:
            pending = m.group(1).strip()
        else:
            continue
        if pending.endswith("\\"):
            pending = pending[:-1].rstrip()
        else:
            lines.append(pending)
            pending = None
    return lines


def run_form(lines):
    """(form, reason): form is 'simple' or 'build-run' with whether FileCheck is used, or a reason"""
    if not lines:
        return None, "no RUN line"
    simple = build = run = False
    filecheck = False
    for line in lines:
        stages = [s.strip() for s in line.split("|")]
        head = stages[0]
        if head in ("%empty-directory(%t)", "mkdir -p %t", "rm -rf %t"):
            continue
        if re.match(r"%target-run-simple-swift\(", head):
            return None, "compile flags"
        if head == "%target-run-simple-swift":
            simple = True
        elif head.startswith("%target-build-swift"):
            if not re.fullmatch(r"%target-build-swift %s -o %t/[\w.-]+", head):
                return None, "compile flags"
            build = True
        elif re.fullmatch(r"%target-codesign %t/[\w.-]+", head):
            continue
        elif re.fullmatch(r"%target-run %t/[\w.-]+", head):
            run = True
        else:
            return None, "other RUN line"
        for stage in stages[1:]:
            if stage == "%FileCheck %s":
                filecheck = True
            elif stage.startswith("%FileCheck"):
                return None, "FileCheck options"
            else:
                return None, "other RUN line"
    if simple and not build and not run:
        return ("simple", filecheck), None
    if build and run and not simple:
        return ("build-run", filecheck), None
    return None, "other RUN line"


def requirement_reason(text):
    for m in re.finditer(r"^\s*//\s*REQUIRES:\s*(.*)$", text, re.M):
        expr = m.group(1).strip()
        if "||" in expr or "!" in expr:
            return f"requires {expr}"
        for term in re.split(r"\s*(?:,|&&)\s*", expr):
            if term and term not in OK_REQUIREMENTS:
                return f"requires {term}"
    for m in re.finditer(r"^\s*//\s*UNSUPPORTED:\s*(.*)$", text, re.M):
        expr = m.group(1)
        for word in PLATFORM_WORDS:
            if word in expr:
                return f"unsupported on {word}"
    if re.search(r"^\s*//\s*XFAIL:.*(\*|macosx|arm64)", text, re.M):
        return "XFAIL"
    return None


def check_directives(text):
    out = []
    for m in re.finditer(r"^\s*//\s*(CHECK(?:-[A-Z]+(?:-\d+)?)?):\s?(.*)$", text, re.M):
        out.append((m.group(1), m.group(2).strip()))
    return out


def classify_text(text):
    """('run', None) for an applicable test, ('run', reason) for one that is not"""
    form, reason = run_form(run_lines(text))
    if reason:
        return "run", reason
    reason = requirement_reason(text)
    if reason:
        return "run", reason
    code = strip_comments(text)
    for m in re.finditer(r"^\s*(?:@testable\s+|@_exported\s+)?import\s+(\w+)", code, re.M):
        if m.group(1) != "Swift":
            return "run", f"imports {m.group(1)}"
    if form[1]:
        for kind, pattern in check_directives(text):
            if kind not in CHECK_KINDS:
                return "run", f"FileCheck directive {kind}"
            if "[[" in pattern:
                return "run", "FileCheck variables"
    for name, regex in FEATURES:
        if re.search(regex, code, re.M):
            return "run", name
    return "run", None


def pattern_regex(pattern):
    parts = re.split(r"\{\{(.*?)\}\}", pattern)
    out = []
    for index, part in enumerate(parts):
        if index % 2:
            out.append(f"(?:{part})")
        else:
            pieces = re.split(r"[ \t]+", part)
            out.append(r"[ \t]+".join(re.escape(p) for p in pieces))
    return re.compile("".join(out))


def filecheck(directives, output):
    position = 0
    previous_end = 0
    pending_not = []
    for kind, pattern in directives:
        regex = pattern_regex(pattern)
        if kind == "CHECK-NOT":
            pending_not.append(regex)
            continue
        found = regex.search(output, position)
        if not found:
            return False
        skipped = output[position : found.start()]
        if kind == "CHECK-NEXT" and skipped.count("\n") != 1:
            return False
        if kind == "CHECK-SAME" and "\n" in skipped:
            return False
        between = output[previous_end : found.start()]
        if any(n.search(between) for n in pending_not):
            return False
        pending_not = []
        position = previous_end = found.end()
    rest = output[previous_end:]
    return not any(n.search(rest) for n in pending_not)


def error_category(stderr):
    m = re.search(r"^[^\n]*?:(?:\d+:)?\s*error:\s*(.*)$", stderr, re.M)
    if not m:
        return "(no error message)"
    text = re.sub(r"'[^']*'", "'_'", m.group(1))
    return re.sub(r"\d+", "N", text).strip()


def crashed(result):
    return result.returncode < 0 or "panicked at" in result.stderr


def run_one(apus, path, tmp, index):
    text = open(path, errors="replace").read()
    exe = os.path.join(tmp, f"t{index}")
    env = dict(os.environ, APUS_CACHE_DIR=os.path.join(tmp, f"cache{index}"))
    try:
        built = subprocess.run([apus, "build", path, "-o", exe], capture_output=True, text=True, timeout=60, env=env)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    if crashed(built):
        return "crash", error_category(built.stderr)
    if built.returncode != 0:
        return "build-error", error_category(built.stderr)
    try:
        ran = subprocess.run([exe], capture_output=True, text=True, timeout=20, cwd=tmp)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    if ran.returncode != 0:
        return "run-fail", f"exit {ran.returncode}"
    form, _ = run_form(run_lines(text))
    if form[1] and not filecheck(check_directives(text), ran.stdout):
        return "check-fail", ""
    return "pass", ""


def verify_checkout(repo):
    suite = os.path.join(repo, SUITE)
    if not os.path.isdir(suite):
        raise SystemExit(f"{suite} is missing: pass --swift-repo with a checkout of swiftlang/swift")
    head = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if head != COMMIT:
        raise SystemExit(f"the checkout is at {head or 'an unknown commit'}, not the pinned {COMMIT}")
    return suite


def collect(suite, needle):
    tests, skipped = [], {}
    for name in sorted(os.listdir(suite)):
        if not name.endswith(".swift") or needle not in name:
            continue
        path = os.path.join(suite, name)
        _, reason = classify_text(open(path, errors="replace").read())
        if reason:
            skipped[reason] = skipped.get(reason, 0) + 1
        else:
            tests.append(path)
    return tests, skipped


def read_baseline(path):
    return {l.strip() for l in open(path) if l.strip() and not l.startswith("#")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--swift-repo", default=os.environ.get("APUS_SWIFT_REPO"))
    ap.add_argument("--apus", default=os.path.join(ROOT, "target/release/apus"))
    ap.add_argument("--filter", default="")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--json", default=os.path.join(ROOT, "tmp/swift_tests.json"))
    ap.add_argument("--results", default=os.path.join(ROOT, "tmp/swift_tests_results.txt"))
    ap.add_argument("--top", type=int, default=15, help="how many first-error categories to print")
    ap.add_argument("--baseline", default=None)
    ap.add_argument("--check", action="store_true", help="with --baseline: fail on a regression or a crash")
    ap.add_argument("--write-baseline", action="store_true", help="with --baseline: write the passing tests to it")
    a = ap.parse_args()
    if not a.swift_repo:
        raise SystemExit("pass --swift-repo or set APUS_SWIFT_REPO")
    suite = verify_checkout(a.swift_repo)
    tests, skipped = collect(suite, a.filter)
    if a.limit:
        tests = tests[: a.limit]
    started = time.time()
    with tempfile.TemporaryDirectory() as tmp, ThreadPoolExecutor(a.jobs) as pool:
        results = list(pool.map(lambda t: (t[1], *run_one(a.apus, t[1], tmp, t[0])), enumerate(tests)))
    seconds = round(time.time() - started, 1)
    counts = {}
    categories = {}
    for _, kind, detail in results:
        counts[kind] = counts.get(kind, 0) + 1
        if kind in ("build-error", "crash"):
            categories[(kind, detail)] = categories.get((kind, detail), 0) + 1
    total = len(results)
    passed = counts.get("pass", 0)
    summary = {
        "commit": COMMIT,
        "suite": SUITE,
        "seconds": seconds,
        "applicable": total,
        "pass": passed,
        "results": counts,
        "not_applicable": dict(sorted(skipped.items(), key=lambda kv: -kv[1])),
    }
    os.makedirs(os.path.dirname(a.json), exist_ok=True)
    json.dump(summary, open(a.json, "w"), indent=1)
    with open(a.results, "w") as f:
        for path, kind, detail in sorted(results):
            f.write(f"{kind}\t{os.path.relpath(path, suite)}\t{detail}\n")
    status = 0
    if a.baseline:
        passing = {os.path.relpath(p, suite) for p, kind, _ in results if kind == "pass"}
        if a.write_baseline:
            with open(a.baseline, "w") as f:
                f.write(f"# {SUITE} tests that apus passes (swiftlang/swift {COMMIT[:9]}); bench/swift_tests.py --write-baseline\n")
                for name in sorted(passing):
                    f.write(name + "\n")
        else:
            base = read_baseline(a.baseline)
            now = {os.path.relpath(p, suite): kind for p, kind, _ in results}
            lost = sorted(n for n in base if n in now and now[n] != "pass")
            new = sorted(passing - base)
            for name in lost:
                print(f"regression: {name} ({now[name]})")
            crashes = sorted(n for n, kind in now.items() if kind == "crash")
            for name in crashes:
                print(f"crash: {name}")
            print(f"baseline: {len(lost)} regressions, {len(new)} newly passing")
            if new and a.check:
                print("(newly passing tests: update the baseline with --write-baseline)")
            if (lost or crashes) and a.check:
                status = 1
    print(f"swiftlang/swift {SUITE} at {COMMIT[:9]}, {seconds} s")
    rate = f"{100 * passed / total:.1f}%" if total else "-"
    print(f"applicable {total}, pass {passed} ({rate}); " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items()) if k != "pass"))
    print("not applicable: " + ", ".join(f"{k} {v}" for k, v in summary["not_applicable"].items()))
    if categories:
        print(f"first errors (top {a.top}):")
        for (kind, text), n in sorted(categories.items(), key=lambda kv: -kv[1])[: a.top]:
            print(f"  {n:4}  {kind}: {text}")
    sys.exit(status)


if __name__ == "__main__":
    main()

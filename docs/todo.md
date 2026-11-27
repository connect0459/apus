# Todo

## Milestones

- [x] M0: benchmark generator (`bench/gen.py`) and baseline against `swiftc -Onone`
- [x] M1: front end
  - [x] Lexer
  - [x] Item splitting and hashing
  - [x] Parser (func, struct, Int/Bool/String literals, if/while/for, calls, member access)
- [x] M2: type checking, Cranelift aarch64 object output, link with the system linker, run
  - [x] Type checker with typed IR (Int, Bool, String, structs, labels, mutability, return paths)
  - [x] Cranelift code generation with overflow and division traps
  - [x] `apus build` command line
  - [x] Benchmark against `swiftc -Onone` (`bench/vs_swiftc.py`)
- [x] M3: per-function cache; editing one function recompiles only that function
  - [x] `compile_function` / `assemble` split, with relocations stored by symbol name
  - [x] Cache key from the typed IR, entry serialization, pack file keeping only the latest build
  - [x] No-op build skip, link skip when the object is unchanged
  - [x] `ld` called directly with the SDK location cached, falling back to `cc`
  - [x] Parallel compilation of the functions that missed the cache
  - [x] Skip parsing and checking of unchanged items, keyed by a signature digest and an item memo
- [x] M4: enum, generics (monomorphization), simple class
  - [x] Enum without payloads: `enum`, `Enum.case`, `==`/`!=`, enum struct fields, exhaustive `switch`
  - [x] Enum with payloads: `Shape.circle(5)`, `case .circle(let r)`, `_` to skip a value
  - [x] Generics by monomorphization: generic functions, structs and enums, inferred type arguments
  - [x] Simple class: reference semantics, memberwise init, mutable members, generic classes
  - [x] Reference counting for classes, with `deinit` and `unowned` members (see the reference counting decision)
  - [x] Optionals (`T?`, `nil`, `== nil`, `!`, `??`, `if let`, `?.`), structs as members of structs and classes, and `weak` class members (see the optional decision)
  - [x] Optional chaining that continues past the first `?.` (`a?.b.c`, `a?.b.m()`, `a?.b!`): the rest of the postfix chain short-circuits and the result is wrapped once
  - [x] `if var`, `while let`, `while var`, `guard let`, `guard var` and `guard` (the binding stays in scope after the `guard`; the `else` body must end every path with `return`)
  - [x] Patterns on optionals in `switch` (`case .some(let v)`, `case .none`), through the enum machinery
  - [x] Downcasts: `as?`, `as!` and `is` on class references, by comparing the class tag with the target class and its subclasses
  - [x] Methods and `mutating` methods on structs, classes and enums; `init` on structs and classes with definite initialization
  - [x] Methods and initializers in generic types
  - [x] Implicit `self` for member reads, member assignments and method calls
  - [x] Generic methods: a method may declare type parameters of its own
  - [x] `mutating` on enums without payloads, `init` on enums
  - [x] Inheritance (single, with `override`, `super` and dynamic dispatch) and `===`/`!==`
- [x] M5: language gaps
  - [x] `break`, `continue` and labels on loops and switches; `guard` may leave with `break` or `continue`
  - [x] Equality on `String` and structs: strings by content, structs member by member (Int, Bool, String, enums without payloads and nested structs only)
  - [x] `switch` on `Int`, `Bool` and `String`: literal cases, `Int` ranges, `default` required except for a `Bool` that covers `true` and `false`
- [x] M6: protocols and type constraints
  - [x] M6a: protocols with method requirements, conformance declared on `struct`, `class` and `enum`, constraints `<T: P & Q>` on functions, methods and types, built-in `Equatable` and `Comparable`
  - [x] M6b: associated types: `associatedtype`, `typealias` bindings in struct, class and enum bodies, `T.Item` in signatures, annotations and members of generic types
  - [x] M6c: protocols as types: `let x: P`, parameters, results and members, optionals of them, requirement calls through a tag comparison chain, `is` and `as?` to a class conformer

- [x] M7: `fatalError()` and `fatalError("message")` as a statement that ends the path
- [x] M8: deep input: a nesting limit as an error, and a large stack for every pass
- [ ] M9: known gaps
  - [x] `Int.min % -1` traps like the division; an enum without payloads prints the name of its case
  - [x] explicit type arguments on function and method calls (`f<Int>(x)`, `box.map<String>(x)`)
  - [x] assignment to a member of a member (`c.a.b = 1`, `s.inner.n += 1`)
  - [x] several initializers per type, chosen by labels and argument types, and delegation with `self.init(...)`
  - [x] generic classes in inheritance: `class D<T>: B<T>`, `class C: B<Int>`, with override, `super` and the casts `is`, `as?`, `as!`, through class descriptors
  - [x] `Character` (a Unicode scalar) and character operations on `String`: `for c in s`, `first`, `last`, `reversed`, `uppercased`, `lowercased`, `prefix`, `suffix`, `dropFirst`, `dropLast`, `replacingOccurrences`, `trimmed`, `firstIndex(of:)`
  - [x] `==` and `!=` on optionals, `Optional` as `Equatable`, and optional members in an `Equatable` struct
  - [x] `deinit` in a generic class
  - [x] several patterns in one `case` that bind the same names (`case .circle(let r), .square(let r):`)
  - [x] labels on payload values (`case rect(width: Int, height: Int)`), required where a case is built and optional in a pattern
  - [x] enum cases without the type name (`.red`, `.circle(5)`, `.some(4)`) where the expected type is known
  - [x] `String` operations: reference counting, `+` and `+=`, `count`, `isEmpty`, ordering and `Comparable`, `String(x)`, `hasPrefix`, `hasSuffix`, `contains` and interpolation
- [ ] M10: collections
  - [x] `Array<T>` / `[T]` with value semantics (copy on write): literals, `count`, `isEmpty`, subscript reads (a bad index traps), `append`, element assignment (`a[i] = x`, `a[i] += x`), `for x in a`, arrays as parameters, results, members and elements of arrays, and generic element types (see the array decision)
  - [x] `removeLast()`, `insert(_:at:)`, `contains`, `first`, `last`, `+`, `+=`, `==` and `!=` (see the array operations decision)
  - [x] `remove(at:)`, a statement like `removeLast()`: it traps outside `0..<count`, releases the removed element and moves the tail down with `memmove`
  - [x] `removeLast()` and `remove(at:)` whose result is used, `popLast()` and `removeAll()` (see the decision on expressions that change an array)
  - [x] elements that are structs, optionals or enums with payloads, stored inline with a stride of their slot count (see the array element decision)
  - [x] assignment to, and changing members of, an element of an element (`g[0][1] = x`, `g[0].append(x)`, `bag.rows[i][j] += s`), at any depth (see the nested element decision)
  - [x] assignment to a member of a struct element (`ps[0].x = 1`, `ps[0].inner.v += 1`, `ps[0].items.append(x)`, `g[1][0].x = 2`), see the element member decision
  - [x] `String.split(separator:)`, `[String].joined(separator:)` and `String` subscripts by scalar offset and range (see the string collection decision)
  - [x] `Dictionary<K, V>` (`[K: V]`) and `Set<E>` with insertion order and copy on write: literals, subscript reads and writes (`d[k] = v`, `d[k] = nil`, `d[k, default: x] += 1`), `count`, `isEmpty`, `keys`, `values`, `removeValue(forKey:)`, `updateValue(_:forKey:)`, `removeAll()`, `for (k, v) in d`, `Set` `insert`, `remove`, `contains` and `for x in s`; keys may be `Int`, `Bool`, `Character`, `String`, enums without payloads, structs of such members, optionals and arrays of keys (see the table decision)
  - [x] changes through a default entry: `d[k, default: []].append(x)`, `insert`, `remove(at:)`, `removeLast`, `removeAll`, element assignment (`d[k, default: [0]][i] = x`) and nested dictionaries (`d[a, default: [:]][b, default: []].append(x)`), see the default entry decision
  - [x] `==` and `!=` on dictionaries and sets, independent of order, and `Equatable` for them when their values are (see the table equality decision)
  - [x] set algebra on `Set`: `union`, `intersection`, `subtracting`, `symmetricDifference`, `isSubset(of:)`, `isSuperset(of:)`, `isDisjoint(with:)` (see the set algebra decision)
  - [x] the mutating set operations `formUnion`, `formIntersection`, `subtract`, `formSymmetricDifference` and `isStrictSubset(of:)`, `isStrictSuperset(of:)` (see the mutating set operations decision)
  - [x] tuples of two to four elements (`(Int, String)`, `(1, "a")`, `t.0`, `let (a, b) = t`, `for (a, b) in pairs`), `zip`, `enumerated()` and `Dictionary(uniqueKeysWithValues:)` (see the tuple decision)
  - [x] tuple labels (`(x: Int, y: Int)`, `t.x`, `(x: 1, y: 2)`) with conversion between labeled and unlabeled tuples where the labels agree, `enumerated()` yielding `(offset:, element:)`, destructuring assignment (`(a, b) = (b, a)`, `(p.x, p.y) = (p.y, p.x)`), tuple patterns in `switch` (`case (0, _)`, `case (let a, let b)`, ranges and strings, several alternatives), and printing and `String(...)` of a tuple (see the tuple labels decision)
  - [x] `if case` with an enum, tuple, value or range pattern, `else` and `else if case` (see the if case decision)
  - [x] condition lists in `if`, `while` and `guard`: Bool expressions, `let`/`var` bindings of optionals and `case` patterns, mixed and separated by commas, so `guard case`, `while case` and several conditions in one `if case` work (see the condition list decision)
  - [x] the `case let .circle(r)` form in `switch`, `if case`, `guard case` and `while case` (see the shared let decision)
  - [x] `where` clauses on the cases of a `switch` (see the where clause decision)
  - [x] `zip` of a `String`, `Set`, `Dictionary`, a range literal or an array, in any combination (see the zip sequence decision)
  - [x] tuple patterns nested in tuple patterns (`case ((0, let y), _)`) and enum patterns inside them (`case (.circle(let r), let n)`), in `switch` and `if case` (see the nested pattern decision)
  - [x] `where` on `for` and after the pattern of `if case`, `guard case` and `while case` (see the where desugaring decision)
  - [x] a tuple of five elements, and parentheses around one name (`let (a) = 4`, `(Int)`) as plain grouping, since there is no one-element tuple (see the tuple size decision)
  - [x] a member of a struct or class value through a default entry: `d[k, default: P()].x = 1`, `.x += 1`, `.inner.x = 3`, `.tags.append("t")` (see the default entry member decision)
  - [x] optional-chain assignment and mutation: `d[k]?.member = x`, `d[k]?.a.b += 1`, `d[k]?.items.append(x)`, `d[k]?.append(x)`, the same on an optional variable or member (`maybe?.x = 5`, `holder.p?.x += 10`), and the optional subscript `d[k]?[i]` (see the optional chain update decision)
- [ ] M11: closures
  - [x] function types `(A, B) -> R`, closure expressions `{ a, b in ... }`, `{ $0 + $1 }` and `{ (a: Int) -> Int in ... }`, trailing closures, and local functions, all non-escaping (superseded: see the escaping closure decision), with variables captured by reference (see the closure decision)
  - [x] closures as arguments of generic functions and methods, with the parameter types taken from the other arguments and the result type inferred from the body
  - [x] `Array`: `map`, `filter`, `reduce`, `forEach`, `contains(where:)`, `first(where:)`, `firstIndex(where:)`, `allSatisfy`, `sorted()` and `sorted(by:)` (a stable merge sort); `Set`: the same members through the keys; `Dictionary`: `mapValues`, `filter`, `forEach`, `contains(where:)`, `allSatisfy` with a `(key, value)` closure, and `Dictionary(grouping:by:)`
  - [x] `compactMap`, `flatMap`, `min()`, `max()`, `min(by:)`, `max(by:)`, `count(where:)`, `sort()` and `sort(by:)` in place, and `reduce(into:)` on arrays (and the non-mutating ones on sets), see the closure members decision
  - [x] an operator as a function value where a closure is expected: `reduce(0, +)`, `sorted(by: <)`, `min(by: <)`, `apply(1, 2, ==)`; the thirteen binary operators `+ - * / % < > <= >= == != && ||` (see the operator function decision)
  - [x] `zip`, `enumerated` and `sort` on a member, an element, an optional chain or a default entry already worked and are covered by tests
  - [x] `prefix(_:)`, `suffix(_:)`, `dropFirst(_:)`, `dropLast(_:)`, `reversed()`, `prefix(while:)` and `drop(while:)` on arrays (new arrays), `prefix(while:)` and `drop(while:)` on `String` (see the library source decision)
  - [x] `lazy` on an array, a `String`, a `Set`, a `Dictionary` or a range, with `map`, `filter`, `compactMap`, `prefix`, `prefix(while:)`, `dropFirst`, `drop(while:)`, `enumerated`, `first`, `first(where:)`, `contains(where:)`, `allSatisfy`, `reduce`, `forEach`, `count`, `isEmpty`, `for ... in` and `Array(...)` / `Set(...)` (see the lazy sequence decision)
  - [x] closures that escape: returned, stored in a field, an array, an optional or a dictionary, passed as a generic argument, and called where they are produced (`adder(1)(2)`, `steps[0](3)`, `button.action(4)`), see the escaping closure decision
  - [x] breaking a reference cycle through a closure with a capture list: `{ [weak self] in ... }`, `{ [unowned obj] in ... }`, several entries and `[self]` (see the weak capture decision)
  - [x] a capture list entry that copies a value: `[x]`, `[x = expr]`, `[weak held = b]` (see the copied capture decision)
  - [x] `weak`/`unowned` on a value that is not a class instance (rejected with a message)
  - [x] generic local functions (`func twice<T>(_ x: T, _ f: (T) -> T) -> T` inside a function, with bounds and explicit arguments) and local functions that call a local function declared after them (see the local function decisions)
- [ ] M12: upstream Swift tests as a specification suite (see the plan below)
  - [ ] M12a: a harness that runs the run-pass tests of swiftlang/swift `test/Interpreter` and reports pass rates
  - [ ] M12b: top-level statements as the program body (an implicit `main`)
  - [ ] M12c: work through the failure categories, most frequent first, with a baseline list that must not regress
  - [ ] M12d: diagnostics tests (`-verify`, `expected-error`), after a `check` command exists

### Plan for M12

**Why.** The Swift repository holds the closest thing the language has to an executable specification. Running it shows which parts of the dialect are missing or wrong, and a list of passing tests keeps them from regressing. The approach follows the one that zust uses on the UI tests of rustc: pin a commit, classify every test from its directives, count only the tests the compiler can be asked to handle, require an exact expectation to pass, and compare against a committed list of passing tests.

**Scope of the first step.** `test/Interpreter` (about 435 files). A test is applicable when all of these hold, and every other test is counted as "not applicable" with its reason, never as a failure:

- Its `RUN:` lines are the plain forms `%target-run-simple-swift` and `%target-build-swift` followed by `%target-run`, optionally piped into `%FileCheck`.
- Its `REQUIRES:` and `UNSUPPORTED:` lines do not exclude macOS arm64 or an executable test, and name no feature the compiler lacks (Objective-C interop, concurrency, sanitizers, resilience, optimization levels).
- It imports no module other than `Swift` (no `Foundation`, `Dispatch`, `StdlibUnittest`).
- It does not use what the dialect cannot express yet: `Double` and the sized integer types, `async`, actors, noncopyable types, macros, `#if`, `@available`, unsafe pointers. These reasons are listed separately so that they read as the backlog of the language.

A rough count with these rules on the pinned commit is 53 of 435 files. About 50 of them rely on top-level statements, so M12b comes before most of the work.

**What counts as passing.** The harness builds the test with apus, runs it, and checks that it exits with 0 and that its standard output satisfies the `CHECK` directives of the file. The checker is a subset of FileCheck: `CHECK`, `CHECK-NEXT`, `CHECK-NOT`, `CHECK-LABEL` and `CHECK-SAME`, with plain text and `{{regex}}` parts, and without variable captures. A test that uses another directive (`CHECK-DAG`, `CHECK-COUNT`, `[[var:...]]`, `--check-prefix`) is not applicable, with that reason. A build error, a crash of the compiler, a timeout and a wrong output are different results, so a crash can be listed first.

**Pinned source.** The harness takes the checkout of swiftlang/swift from an option or an environment variable, refuses to run when it is not at the pinned commit, and never writes a local path into the repository. The commit is recorded in the baseline file.

**Regression list.** `bench/swift_tests_passing.txt` lists the passing tests (one path per line). `--check` fails when a listed test stops passing or when the compiler crashes on any test, and `--write-baseline` rewrites the list. A test that passes without testing what it names, for example because it only prints something trivial, goes to an exclusion list next to it, as zust does.

**Working loop.** After the harness exists, the first error of every failing applicable test is grouped into categories. One category at a time is fixed, each change is kept only if no test in the baseline regresses and `just verify` still passes. Language divergences that this exposes are recorded as decisions here: a dialect feature that Swift does not have stays documented as an extension, and a leniency that Swift rejects is decided case by case (it is a false pass for the diagnostics tests of M12d).

**Not in this plan.** Diagnostics tests (2440 files with `expected-error`) need a `check` command that stops after type checking, errors that carry a column, and a way to match messages; the line of the error alone gives many accidental passes. They are M12d. `StdlibUnittest` tests, tests of the standard library outside the subset, and anything that needs Foundation are out of scope until those exist.

**Open questions.**

- Top-level statements: how they relate to `func main()` (the entry point decision under Open decisions). The likely rule is that a source without `func main()` whose items include top-level statements gets an implicit `main`, with variables declared at the top level treated as locals of it.
- Whether the harness lives in `bench/` as a Python script (like `bench/gen.py`) or as a Rust test binary. A script is simpler and mirrors zust; the baseline check can still run in CI.

## Open decisions

- Rebuild timings were last measured on 2026-11-27 with `just bench-vs-swiftc` (10 cores; the load average was 6.6 because of the endpoint security daemons of the machine, so the figures are indicative; best of 5 runs for apus and 3 for `swiftc -Onone`; wall ms):

  | | gen1000 apus | gen1000 swiftc | gen5000 apus | gen5000 swiftc |
  | :--- | ---: | ---: | ---: | ---: |
  | cold build | 83 | 2390 | 257 | 11566 |
  | edit 1 function | 84 | 2461 | 201 | 11539 |
  | edit a comment | 42 | | 151 | |
  | no change | 4 | | 7 | |

  The question that the 2026-11-20 table raised (editing one function costs almost as much as a cold build) is answered in the decision "A rebuild is bounded by the linker and the first read of a new file". Remaining costs of a one-function edit on gen1000, in order: `ld` about 30 ms with the minimal library stub (45 ms before), the first read of the freshly written source about 21 ms (the operating system, not apus), lexing and splitting about 7 ms, loading the memo and the pack about 4 ms, and the change itself (parse, check, compile) under 3 ms. Candidates that are not done: write the Mach-O executable without `ld` (removes the 30 ms floor, but needs chained fixups and an ad-hoc code signature), a faster lexer, loading the pack lazily.
- Statements end at a newline, `;` or `}`; a binary operator must stay on the same line as its left operand.
- Wrapping operators (`&+`, `&*`) and overflow trapping are not decided.
- Entry point is `func main()` instead of top-level statements.
- Linking cost with the system linker on macOS: measured on 2026-11-27, see the decision "A rebuild is bounded by the linker and the first read of a new file".
- Coverage gate stays at 80% line / 90% function; branch coverage is not gated because the stable toolchain cannot measure it. Baseline on 2026-11-20 (`cargo llvm-cov`): 95.9% lines and 93.6% functions. The weakest files are `link.rs` (50% of functions: the fallback and error paths of the system linker) and `codegen.rs` (75% of functions: the error conversions). Compare against these numbers when a change lowers coverage.
- Known gaps in the M2 language: `String` has no `split(maxSplits:)`, no `omittingEmptySubsequences:` and no open-ended range subscript, `append`, `insert` and `removeAll` are statements of their own and produce no value, an array has no slicing, a struct with an array member cannot be `Equatable`, a closure that captures strongly the variable that holds it is a reference cycle that leaks, an operator is not a function, its `count` is the number of UTF-8 bytes while its character operations count scalars, a `Character` is a Unicode scalar and not a grapheme cluster, its tests and case changes know only ASCII, a value of a struct, a class or an optional cannot be converted to `String`, an enum with payloads cannot be printed, `switch` works only on enums, `Int`, `Bool` and `String` and a case value must be a literal, generic arguments cannot be structs or enums with payloads where they end up as a struct field or a payload, a value of a type parameter without a constraint can only be copied, passed, returned and stored, an expected type reaches only the top expression of a `let` initializer or a `return`, payloads at the same position of different cases must have one type when it is a class or a struct, an initializer cannot be overridden or inherited so a subclass of a class with an `init` must declare its own, `super.init` builds a separate superclass instance and copies its members so a method called from a superclass initializer dispatches on the superclass, `super` is accepted only as `super.method(...)` and `super.init(...)`, implicit `self` applies to a name only inside a method or an initializer and not to a member of another value, an initializer cannot have type parameters, a mutating method of an enum without payloads must be called as a statement of its own, once per statement, on a variable, an initializer counts only top-level `self.member = value` statements whose value does not mention `self`, a member path that crosses an `unowned` or `weak` member cannot be assigned through, `unowned` applies only to a class member and not to a local, a parameter or a struct member, `weak` applies only to a `var` class member of an optional class type, a cycle of strong references leaks, a variable is released at the end of its scope and not after its last use, an argument is borrowed for the call so a callee that drops the last other reference to it is undefined behavior, and a `case` may repeat a pattern without a warning.

## Decisions

### Variadic libc calls use the Apple arm64 stack convention (2026-11-20)

**Problem.** `print` of an `Int` calls libc's variadic `printf("%ld\n", value)`. Cranelift has no variadic calls, and on Apple arm64 every variadic argument is passed on the stack, so a plain two-parameter declaration passes the value in a register that `printf` never reads.

**Options.** Generate an integer-to-decimal routine in Cranelift IR (no variadic call, but code in every program); declare `printf` with padding parameters so the value lands on the stack (one declaration, one call); compile a small C runtime and link it (adds a toolchain step and a second artifact).

**Decision.** Declare `printf` with nine `i64` parameters: the format, seven padding values, then the value. The ninth integer argument goes to the first stack slot, which is where a variadic callee reads its first variadic argument. `Runtime::declare` and `Emitter::print_int` in `src/codegen.rs` implement it, with `VARIADIC_PADDING` as the constant. `print` of a `Bool` or `String` calls `puts`, which is not variadic.

**Consequences.** `print` of an `Int` needs no runtime support code. The trick is correct only on Apple arm64, and a port to another target must replace it. The end-to-end tests that print integers validate it.

**Reference.** [Writing ARM64 code for Apple platforms](https://developer.apple.com/documentation/xcode/writing-arm64-code-for-apple-platforms) documents that variadic arguments are passed on the stack.

### Function cache keyed by typed IR (2026-11-20)

**Problem.** After an edit, only the functions affected by it should be recompiled, and nothing should be linked when the object did not change.

**Options.** Key by the item hash plus the signatures it references (also skips parsing and checking of unchanged items, but needs a way to find which names an item references from its tokens); key by a hash of the typed IR (the front end still runs on the whole file, but correctness follows from the key covering everything code generation reads).

**Decision.** Key by a stable FNV-1a hash of the typed IR, with struct types encoded by their field types and a stamp identifying the compiler (version, size and modification time of the executable). Entries hold machine code, alignment, relocations that name their targets by symbol, and the string literals used, so they do not depend on one build's numbering of functions and data. They live in one pack file under `.apus-cache/` next to the output (override with `APUS_CACHE_DIR`), which keeps only the entries the latest build used.

**Consequences.** Editing a callee's body does not recompile its callers, because the callers' typed IR is unchanged; changing a callee's result type does. The front end still lexes, parses and checks the whole file on every build that changes the source. Keys are 64 bits, so a collision is possible in principle; for thousands of functions its probability is around 10^-12, which is acceptable for a development cache but belongs in the threat model for cache poisoning. A corrupt or truncated cache file is ignored as a whole.

### Unchanged items skip parsing and checking through a signature digest (2026-11-20)

**Problem.** The front end lexed, parsed and checked the whole file on every build that changed the source, although a comment edit or a one-function edit leaves most typed IR unchanged.

**Options.** Track which names each item references and invalidate only its dependents (precise, but needs a reference extractor over tokens and a dependency graph); hash every function header and struct definition into one signature digest and trust a memoized result for an item only while the digest is unchanged (conservative, needs no reference tracking).

**Decision.** `signature_digest` hashes the header tokens of every function and the whole token stream of every struct, in source order. `ItemMemo` (the `items` file under `.apus-cache/`) maps an item hash to the function key of its typed IR, and is written only after a successful build. When the memo's compiler stamp and digest match, a function whose item hash is known and whose entry is in the cache is neither parsed nor checked. If any function is dirty, the other functions are parsed with `parse_signature`, which stops at the function header, so `Signatures` is built without reading their bodies; only dirty functions are fully parsed and checked.

**Consequences.** A comment edit checks nothing, and a body edit checks only that function. Any header or struct change, adding or removing a function, and reordering invalidate every memoized item, so those builds check everything as before. A body that was valid under an unchanged digest cannot become invalid, so skipped items need no re-check. The remaining front-end cost is lexing, splitting and hashing the file, plus a header parse per clean function when something is dirty.

### A switch on an enum lowers to an if chain in the checker (2026-11-20)

**Problem.** `switch` over a payload-less enum needs code generation, a cache key encoding and an exhaustiveness rule.

**Options.** Add `TStmt::Switch` and emit a Cranelift jump table (a new typed IR node in `key` and `codegen`); lower the switch in the checker to a temporary local plus a chain of `TStmt::If` on the existing `==` for integers.

**Decision.** An enum value is a 64-bit integer holding the case index, `TExpr::EnumCase` is its only new typed IR node, and the checker lowers `switch` to `let tmp = subject` followed by nested `If` statements. When the switch has no `default`, the last case becomes the final `else`, which is safe because exhaustiveness is checked first. A `default` must be the last case.

**Consequences.** `key` encodes an enum type as one tag, so renaming an enum or adding cases it never selects does not change a function's key; adding or reordering cases still invalidates through the signature digest. A switch costs a linear chain of comparisons, which is acceptable for a non-optimizing backend and will matter only for enums with many cases. The subject is evaluated once. Payload enums will need a new representation (tag plus slots) and a pattern-binding construct, so this lowering will be revisited then.

### A payload enum is a layout struct of one tag slot and the widest payload (2026-11-20)

**Problem.** An enum whose cases carry values needs storage, copying, parameter passing and result passing, and the typed IR should not grow a second aggregate kind.

**Options.** Add a tagged-union type to the typed IR and the backend; reuse the struct machinery with a synthesized layout.

**Decision.** An enum with at least one payload becomes `Ty::Struct` over a synthesized layout of `1 + widest payload` slots of 8 bytes. Slot 0 holds the case index and slot `1 + n` holds the nth payload value. All slots are typed `Int` in the layout, so a copy moves whole slots whatever case they hold; a value of another type is read back with the type recorded in the `Field` node. Construction is `TStruct`-style `StructInit` with the case index first, and the checker lowers `switch` as before, reading the tag with `Field` and binding payload values with `Let` statements at the start of the case body. Enums without payloads stay scalar (`Ty::Enum`). The layout is registered after the declared structs, so their ids do not move.

**Consequences.** Copy, parameter, result and cache key code is unchanged, and the key encodes a payload enum as a struct of `Int` slots. A value occupies the widest case's size even for `empty`. Equality, printing and member access on a payload enum are rejected. Payloads cannot be structs or payload enums, which would need nested layouts and recursion rules, and a struct field cannot hold a payload enum for the same reason. The bound names are immutable copies of the payload.

### Generics are checked once with opaque type parameters and monomorphized by substitution (2026-11-20)

**Problem.** Generic functions, structs and enums need a checking model, a representation per instantiation, and a place in the function cache and the item memo, without breaking the rule that a callee's body edit does not invalidate its callers.

**Options.** Check a generic body again for every instantiation with concrete types (C++ templates: permissive, uninstantiated bodies unchecked, operators on type parameters allowed); check it once with each type parameter opaque and substitute types in the typed IR (Swift without protocols: sound, uninstantiated bodies checked, no operators on type parameters).

**Decision.** `Ty::Param(i)` is an opaque type. A generic function is checked once into a typed IR that mentions `Param` and carries `type_args` on each call. `Signatures` (`src/sigs.rs`) interns struct and enum instances by definition and type arguments, so `Box<Int>` is a layout of its own and `Box<T>` inside a generic body is a symbolic instance. Type arguments are inferred by unifying parameter types with argument types, and for a `let` annotation or a `return` the expected type seeds the unification. `mono` (`src/mono.rs`) turns each call with concrete `type_args` into an instance: it substitutes the types of the generic typed IR, names the result `name<Type,...>` and rewrites calls transitively, with one instance per name. An instance is an ordinary function, keyed by its typed IR like any other. Generic function items hash their whole text into the signature digest, and the item memo keeps, for each non-generic function, the keys of its own entry and of every instance it needs.

**Consequences.** Editing a generic function body rechecks every function, but only instances whose typed IR changed are recompiled. Editing a non-generic function leaves the instances of other callers in the cache and the object. Instances are compiled once however many callers use them. A type parameter supports no operators until protocol constraints exist. A generic argument that becomes a struct field or an enum payload must not be a struct or an enum with payloads; the error is reported where the instance is created. Instance names are readable (`id<Int>`), so a user function can never collide with one.

### A class is a heap block behind a pointer and is never freed (2026-11-24)

**Problem.** A class needs reference semantics, so its lifetime must be managed somehow, and the choice touches the checker, the typed IR, the cache key and code generation.

**Options.** Leak (`malloc` only); reference counting like Swift; one program-wide arena. The language has no `deinit`, `weak`, `unowned` or `isKnownUniquelyReferenced`, so no program can observe the difference between leaking and reference counting except through memory use. Reference counting would also turn every copy of a struct that holds a class reference from a slot move into a per-field retain, which rewrites the copy, parameter, result and key machinery that structs and payload enums share.

**Decision.** `Ty::Class(StructId)` is a type of its own that shares the struct layout table: one 8-byte slot per member. A class value is a 64-bit pointer, so it is copied, passed and returned like an `Int`, never through the struct memory path. `TExpr::New` allocates with libc `malloc` and stores the members; nothing frees. Member reads and `AssignField` work on the pointer unchanged, and a class member may be assigned through a `let` binding or a parameter when the member is `var`. The key encodes a class with its own tag, so a class and a struct with the same members never share a key.

**Consequences.** Every copy of a class reference is visible in the typed IR as a value of `Ty::Class`, and allocation is the single `New` node, so a later lowering pass can insert retain and release without changing the checker. Introducing `deinit` or `weak` is the point where leaking stops being observably correct and reference counting must replace it. Programs leak every instance, which is acceptable for a short-lived compiler experiment. `==` and printing are rejected for classes.

### A member of a type is an item of its own and lowers to a function (2026-11-24)

**Problem.** Methods and initializers are written inside the type body, but a type item is hashed as a whole and its tokens feed the signature digest, so editing one method body would have invalidated every memoized function and defeated incremental builds.

**Options.** Keep members inside the type item and recheck everything on a method edit; split the members out of the type item in `items` and give each its own hash and signature.

**Decision.** `split_items` removes every `func`, `mutating func` and `init` at brace depth 1 of a struct, class or enum and emits it as an `ItemKind::Method` named `Type.member`. The owner is part of the member's hash, because the same text in two types is two functions; the header (everything before the body) is the member's signature hash, so the digest changes when a method is added, removed or re-declared but not when its body is edited. A member parses as a `FuncDecl`, and the driver attaches the owner. In `Signatures` a method is the function `Type.method` whose first parameter is `self`, an initializer is `Type.init` returning the type, and a call through a value (`x.m(a)`) is lowered by the checker to a plain `Call` with the receiver first, so the typed IR, the key and the backend know nothing about methods. A struct or an enum with payloads is already passed by address, so a `mutating` method receives the caller's own storage and writes through it; the receiver must be a `var` variable (or `self` in a mutating method), never a temporary. A class receiver is a pointer, so a class method may change members without being `mutating`.

An initializer starts from a zero-filled `self` (`TExpr::Zero` members in a `StructInit` or a `New`), runs its body and returns `self`. `init_check` rejects an initializer unless top-level `self.member = value` statements, whose values do not mention `self`, set every member before any other use of `self` and before any `return`; a `let` member is assignable inside an initializer only. A type with an `init` has no memberwise initializer.

**Consequences.** Editing a method body checks and compiles only that method, and the callers keep their cache entries. Adding, removing or re-declaring a method rechecks every function, like a function header change. Zero values are never observable because of the initialization rule. Generic types and generic methods are rejected because a method of `Box<T>` would need to be generic over the type's parameters and its self type inferred at the call; enums without payloads are plain 64-bit values with no storage to write through.

### A member of a generic type is a generic function over the type's parameters (2026-11-24)

**Problem.** A method of `Box<T>` must be checked once, instantiated per type argument, and called through a value whose type already fixes the arguments.

**Options.** Give the method type parameters of its own and repeat the type's arguments at every call; treat it as a generic function whose parameters are the type's, so the existing check-once-and-substitute model and `mono` apply unchanged.

**Decision.** In `Signatures`, a member of a generic type has the type's parameters as its `generics`, and its `self` parameter (or the result of an `init`) is the identity instance `Box<T>`. A call through a value (`b.get()`) unifies the parameter types, receiver included, with the argument types, so `Box<Int>` against `Box<T>` fixes `T`, and records the result as `type_args` on an ordinary `Call`. `Type<Int>(...)` and `Type(...)` reach an initializer through the same inference, with explicit arguments checked against the substituted parameter types. `mono` instantiates `Box.get<Int>` like any generic function, and `items` marks the members of a generic type as generic, so their whole text joins the signature digest and the driver hands them to the instantiator.

**Consequences.** Editing a method body of a generic type rechecks every function, as editing a generic function does, but only instances whose typed IR changed are recompiled, and editing a caller leaves the instances in the cache. A method of a generic type cannot yet declare type parameters of its own, because a call would have to infer both groups. The type parameter supports no operators, as before. A mismatch after the receiver has fixed a parameter is reported with the concrete type, so `b.set(true)` on `Box<Int>` says `expected Int`.

### A name that is not in scope falls back to a member of self (2026-11-24)

**Problem.** Swift code reads and assigns members and calls sibling methods without writing `self.`, and the initialization rule must keep working when the member is named bare.

**Options.** Rewrite the syntax tree before checking, replacing every bare member name with `self.name`; resolve in the checker only when the ordinary lookup fails.

**Decision.** The checker resolves a name in this order: a local or a parameter, then a member of `self`. A bare call prefers a method of the enclosing type over a function of the same name, as Swift does. All three forms (read, assignment target, call) reuse the explicit paths, so mutability, `mutating` and `let` rules are identical to writing `self.`. `init_check` now tracks scopes: the parameters and the locals declared so far shadow a member, a nested block shadows only inside itself, and a bare member name or a bare method call counts as a use of `self`, while `name = value` at the top level initializes `name`.

**Consequences.** A local declared later in a block does not shadow an earlier use, which is what the checker does too, so the two agree. A member is visible by its bare name only inside the type's own methods and initializers. The rule is conservative in one direction only: an initializer that Swift would accept may be rejected, never the reverse.

### A method's own type parameters follow those of its type (2026-11-25)

**Problem.** `func map<U>(...)` inside `Box<T>` needs both groups of parameters inferred at one call, and a plain type's method with `<U>` must still be instantiated and keyed like a generic function.

**Options.** Nest two substitution levels (type arguments fixed by the receiver, then method arguments); append the method's parameters to the type's and reuse the single flat inference.

**Decision.** In `Signatures` the `generics` of a member are the type's parameters followed by the method's, so `Ty::Param(i)` for `i` below the type's count still names the type's parameters and the identity instance of `self` is unchanged. A call through a value unifies receiver and arguments as before, and `mono` sees one flat list. A method parameter that repeats a name of its type is rejected, and so is any type parameter on an `init`, because `Type<Int>(...)` gives explicit arguments for the type only. `items` marks a member generic when its type is generic or its own header has `<` after the name, so its whole text joins the signature digest.

**Consequences.** Editing the body of a generic method rechecks every function, and only instances whose typed IR changed are recompiled; editing a caller reuses the instances. A type parameter that appears only in the result cannot be inferred and is reported as such. Explicit type arguments on a method call are still not accepted.

### An enum initializer assigns self, and a scalar enum is mutated through a one-slot cell (2026-11-25)

**Problem.** An enum without payloads is a 64-bit value in a register with no storage to write through, so `mutating` has nothing to receive, and `init` has no members to initialize.

**Options.** For `mutating`: make the method return the new `self` and let the caller reassign (no new storage, but it competes with the method's own result); pass the address of a one-slot cell, as structs are passed, and let the caller copy the value back (the model of structs and payload enums). For `init`: reuse the member rule with a single pseudo-member `self`.

**Decision.** An initializer of an enum starts from a zero value and must assign `self` on every path before any other use of it: a top-level `self = value`, or an `if`/`else` whose branches both do (`check_enum_initialization`). A payload enum starts from a zero layout, a scalar enum from `Zero`. `E(...)` and `E<T>(...)` reach `E.init` as they do for structs. For `mutating` on a scalar enum, `Signatures::enum_cell` interns a one-slot struct layout per enum and the method's `self` parameter has that type. The callee copies the slot into a mutable local `self`, and writes it back at the end of the body and before every `return` (a `return` with a value is evaluated first). The caller wraps the call in `let cell = [x]`, the call, and `x = cell[0]`; only a statement consisting of the call may do this, and only one such call per statement, because the write-back cannot be placed inside an expression.

**Consequences.** The typed IR, the key and the backend gain nothing new: the cell is a struct and the write-back is an `Assign`. A statement like `let n = flag.flip()` or `if x.next() > 0` is rejected until expressions can carry statements. The cell adds a stack copy per call, which is acceptable for a non-optimizing backend. A generic enum without payloads cannot use `mutating` because its type arguments cannot be inferred from a cell.

### Single inheritance lays out the superclass first and dispatches on a class tag (2026-11-25)

**Problem.** A subclass must be usable wherever its superclass is expected, share the superclass members, and run the override of the actual class when a method is called through a superclass type, without giving up the typed IR that the function cache keys on.

**Options.** For dispatch: a vtable in a data symbol with indirect calls (new relocations, a table per class, function addresses in the IR); a hidden class tag in every object and a compare-and-call chain in code. For `super.init`: initialize the superclass part in place (needs a second entry point per initializer); build a separate superclass instance and copy its members.

**Decision.** Every class object starts with a hidden slot `$class` that holds the class's struct id, followed by the superclass members, then the class's own. Member indices of a superclass are therefore valid in every subclass, and `Ty::Class(sub)` is accepted wherever `Ty::Class(super)` is expected (`Signatures::is_subclass`, used by `expect`). A method call resolves the nearest implementation up the chain (`resolve_method`). When a strict subclass of the static receiver type implements it differently, the checker emits `TExpr::Virtual`: the codegen loads the tag, compares it with each overriding class and calls that implementation, falling back to the static one. `super.method(...)` is a plain `Call` to the superclass implementation. `override` is required exactly when a superclass has the method, and must keep the parameters, labels and result. A subclass initializer must set its own members, then call `super.init(...)` at its top level, and may use `self` only after it; `super.init` is lowered to a call of the superclass initializer (or memberwise initializer) followed by member copies into `self`. `===` and `!==` compare two references of related classes by address and reuse the integer comparison.

**Consequences.** A virtual call costs a load and a compare per overriding class, which is linear in the number of overriders and acceptable for a non-optimizing backend; a real vtable can replace `Virtual` later without touching the checker rules. Adding or changing an override changes the signature digest, so every function is checked again, while editing an override body checks and compiles only that body. The typed IR of a caller lists the overriders, so adding a subclass changes the key of every function that calls the method through the superclass type. Generic classes are excluded because a subclass of `Box<T>` would need the superclass arguments to be part of the layout and the tag.

### Classes are reference counted by a pass over the typed IR (2026-11-25)

**Problem.** Leaking every instance made `deinit`, `unowned` and bounded memory impossible, and structs that hold class references copied them with a plain slot move.

**Options.** Emit retain and release in code generation (no new IR, but variable lifetimes and temporaries would live inside the Cranelift block structure); insert them in a pass over the typed IR (the function key sees the counted code, and the backend learns only primitives). For the destructor: one program-wide function that switches on the class tag (needs every class, including generic instances found only in checked bodies, at every build); the address of a per-class drop function in each object (found by scanning the `New` nodes of the functions that were rechecked).

**Decision.** Every class object starts with four hidden slots: class tag, strong count (a new object holds 1), the address of its class's drop function, and the unowned count. `rc::insert` runs after monomorphization and before keying, on every non-generic function and every instance. A value is owned (`New`, a call, `StructInit`, `Retain`, a read of an unowned member) or borrowed (a local, a field). Consuming positions (`let`, assignment, member assignment, constructor arguments, `return`) take an owned value and wrap a borrowed one in `Retain`; call arguments and operands are borrowed, and an owned value in such a position is bound to a temporary (`Bind`) that is released after the statement, or before the branch for an `if`, `while` and `for` header. Parameters are not owned by the callee. A local owned value is released, in reverse order of declaration, at the end of its block and before every `return`, which evaluates its result first; released locals are cleared, so a release of a variable that was never assigned, or already released, does nothing. Assigning evaluates the new value, releases the old one, then stores. A struct with class members is retained and released member by member, from the layout. `Release` decrements, and the last one calls the drop function in the header. `Signatures::construct` and the pass fill the tag and drop slots, because a generic class only knows its concrete type after monomorphization. `rc::drop_function` builds, as typed IR, the function `Class.$drop`: it marks the object as being destroyed, runs `deinit` from the class up through its superclasses, releases the members, marks the count as 0 and frees the memory unless unowned references remain. The driver builds one drop function for every class that a rechecked function creates and records its key with that function in the item memo.

An `unowned` member does not retain: assigning it counts an unowned reference, reading it takes a strong reference and traps if the count is 0, and releasing the last unowned reference to a deinitialized object frees its memory. `super.init` moves the members of a temporary superclass instance into `self` and frees it without running its `deinit`.

**Consequences.** The typed IR gains `Retain`, `Bind`, `Moved`, `Release`, `Clear`, `Free`, `FuncAddr` and the unowned counterparts; the backend gains inline retain and release and one indirect call. Every object is 32 bytes larger. A release costs a branch on null, a load and a compare, and an object that is freed costs an indirect call. A cycle of strong references still leaks, and `weak` waits for an optional type. A method called from a superclass initializer through `super.init` dispatches on the temporary superclass instance. The superclass-first layout of inheritance keeps working because the drop function walks the same chain.

### Optional is a builtin generic enum, and a nested struct is stored inline (2026-11-25)

**Problem.** `weak` has to read as nil, which needs an optional type; an optional member of a class or a struct (`var next: Node?`, `weak var parent: Parent?`) needs structs inside structs, which the layout did not allow, and a class could not name itself.

**Options.** For the type: a nullable pointer for classes only (nothing to store, but `Int?` is missing); a builtin generic enum `Optional<Wrapped>` with cases `none` and `some`, reusing the payload enum machinery, generics and monomorphization. For nesting: box every struct member in the heap; store it inline and flatten the layout.

**Decision.** `Optional<Wrapped>` is registered first by `Signatures::collect`, `T?` is parsed as `Optional<T>`, and the type prints as `T?` in messages. A payload slot of an enum has the type of the payloads that use it (all cases must agree when it is a class or a struct, otherwise it is an `Int` slot), so classes and structs are counted by the reference counting through the layout. `none` fills the slots with `Zero`. The checker inserts the `some` wrapper where a value meets an optional type (`coerce`: a `let` annotation, an assignment, a `return`, an argument, a member initializer, a payload) and gives `nil` its type from the expected type; `x == nil` compares the tag, `x!` is `Unwrap` and traps on nil. `??`, `?.` and `if let` bind the subject once, with `Bind`, and use `Select`, a conditional expression whose two arms are owned values. A struct member is stored inline: `TStruct` records for each declared member the first slot it occupies and the flattened slot types, and the IR keeps member indices. Reading a struct member yields the address inside the parent, so a `Bind` of a struct copies it. A class registers its id before its members are resolved, so it can refer to itself.

A `weak` member has the type `Optional<Class>`, must be a `var`, and counts through the same unowned count slot as `unowned`: assigning counts a reference to the new target and releases the old one, reading yields an owned optional that is nil when the strong count is 0, and the drop function releases it as a weak reference. The memory of a dead target is kept until its last weak or unowned reference goes.

**Consequences.** An optional of a class costs two slots, not one. A member of a member cannot be assigned in place. A function that reads a weak member holds a strong reference for the statement. A cycle that does not use `weak` or `unowned` still leaks.

### An optional chain is a subject and a link, and a cast tests the class tag (2026-11-25)

**Problem.** `a?.b.c` must short-circuit as a whole and produce one optional, a `guard let` binding must outlive the statement, and a downcast needs a runtime test that works with the reference counting.

**Options.** For chains: give each `?.` its own optional result (`a?.b?.c` is then required); parse the rest of the postfix expression as one link whose base is a placeholder. For casts: store the class name in each object; compare the class tag that dispatch already uses.

**Decision.** `ExprKind::OptionalChain` holds the subject and a link whose base is `ChainValue`; a nested `?.` inside the link starts a nested chain. The checker evaluates the subject once with `Bind`, pushes the unwrapped value for `ChainValue`, checks the link, and wraps the result once (an optional result is not wrapped again), through `Select`. As a statement, a chain becomes an `if` on the tag, so a call that returns nothing can be chained. `guard let` lowers to a hidden holder, an `if` on the tag whose body is the `else` body, and a binding declared in the current scope; `require_exit` accepts the `else` body only when every path returns. `while let` keeps a flag and re-evaluates its subject at the top of every iteration. A `switch` is lowered inside its own block, so its subject is released where the `switch` ends. `is`, `as?` and `as!` bind the operand once and compare its class tag with the tags of the target class and of every subclass (`Signatures::class_tags`); a cast between unrelated classes is rejected. `as?` yields `Optional<Target>`, `as!` unwraps it and traps on nil, an optional operand is looked through, and casts bind tighter than `??` and looser than ranges. A local declared with a type annotation now has the annotated type, which a subclass instance assigned to a superclass annotation needs for casts to see the declared type.

**Consequences.** A cast costs one comparison per class in the family of the target. Casts work on class references only: no protocols, no `Any`, no casts of values. A mutating call inside a chain is rejected because the unwrapped value is a copy. `guard` accepts `return`, `break`, `continue` and `fatalError` as a way out.

### break and continue are depth-indexed jumps over loops and switch blocks (2026-11-25)

**Problem.** `break` and `continue` must leave scopes that own reference-counted locals, re-run a `while` condition that owns temporaries, and, as in Swift, `break` inside a `switch` must leave the switch.

**Options.** Lower to flag variables in the checker (no new IR, but every later statement is wrapped in an `if`); add jump nodes to the typed IR and resolve targets in the backend.

**Decision.** `TStmt::Break(depth)` and `TStmt::Continue(depth)` count the enclosing breakable constructs outward from zero. The breakable constructs are `While`, `ForRange` and the new `TStmt::Block`, which a `switch` lowers into. The checker resolves labels to depths (`jump_depth`); an unlabeled `continue` skips switches, and `continue` cannot name a switch label. `rc::insert` releases the locals of every scope above the target before the jump, and a `continue` in a `while` whose condition owns temporaries first re-evaluates the condition flag. Codegen keeps a stack of exit and continue blocks; a `for` loop continues at a latch block that increments the variable.

**Consequences.** The key encodes the depth, so moving a `break` out of a nested loop changes the key. `guard` accepts a `break` as its way out only when it leaves the guard's own scope: a `break` inside a `switch` nested in the `else` body does not count. Labels apply to loops and switches only, and a label cannot be reused by an enclosing statement.

### String equality is an inline byte loop and struct equality is a chain of member comparisons (2026-11-25)

**Problem.** `==` on `String` and on structs was rejected, and a literal is a NUL-terminated byte sequence with no length.

**Options.** For strings: call libc `strcmp`; emit a comparison loop in Cranelift IR. For structs: synthesize a function per struct; lower the comparison in the checker to the existing `==` on each member.

**Decision.** `TExpr::Binary` with `Eq` or `Ne` on two `Str` operands compiles to an inline loop that compares bytes until they differ or both reach the NUL. A struct comparison is lowered by `struct_equality` into `&&` over the leaves of the member tree, comparing `Field` paths of two `Bind` temporaries (the first leaf binds the operands, so each is evaluated once and left before right). Leaves are `Int`, `Bool`, `String` and enums without payloads; nested structs are flattened, while a class, a payload enum (including an optional) or a type parameter is rejected with the member's name.

**Consequences.** No runtime support code and no new typed IR node, so the cache key is unchanged; every string comparison costs a small loop at its use site. A struct with no members compares as `true` without evaluating its operands. A string cannot contain NUL, so the loop is exact.

### A switch on a scalar compares the subject with literal cases (2026-11-25)

**Problem.** `switch` accepted only enums, whose cases are patterns over a closed set; `Int`, `Bool` and `String` need value cases and a different exhaustiveness rule.

**Options.** A new typed IR node with a jump table; lower to the same if chain as enums, comparing the subject with each case value.

**Decision.** `CaseLabel::Values` holds expressions: literals (negative `Int` included) and, for an `Int` subject, ranges (`a...b`, `a..<b`). The parser reads a case as values when it starts with a number, a string, `-`, `true`, `false` or a name that is not followed by `.`. `value_switch` binds the subject once, tests `==` for a literal and `>=`/`<=`/`<` for a range, and shares the chain construction with enum switches (`chain_branches`). A case value that is not a literal is rejected, so a case never evaluates code. `Bool` is exhaustive when `true` and `false` both appear; `Int` and `String` need `default`. A literal of another type and an enum pattern on a scalar are rejected, and so is a value case on an enum.

**Consequences.** No change to the typed IR, the key or the backend. A switch costs a linear chain of comparisons, and a string case costs a byte loop. Overlapping cases are accepted and the first one wins, with no warning. A `case let` binding or a `where` clause is not supported.

### A protocol requirement on a type parameter is a Witness node resolved by monomorphization (2026-11-25)

**Problem.** A generic body is checked once with opaque type parameters, so a call such as `x.area()` or `a == b` on a `T` has no callee until the type is known, and the typed IR, the cache key and the backend should stay free of dynamic dispatch.

**Options.** Lower the call to a pseudo function `P.area` and let `mono` rename the callee (no new IR, but operators on `Int` need an expression and not a call); add a `TExpr::Witness` node that `mono` replaces by a call, an operator or a virtual call.

**Decision.** `protocol P { func m(...) -> R }` declares method requirements, including `mutating` ones, whose types may mention `Self` (a `Ty::Param(0)` in the protocol's own scope). A type declares conformance after its name (`struct S: P, Q`; for a class the first name is the superclass when it is not a protocol). `Signatures` keeps the protocol table, and `check_conformances` verifies for every declared protocol that each requirement has a method of the same labels, parameter and result types (with `Self` replaced by the type) and `mutating` flag; a generic method cannot satisfy a requirement. A constraint `<T: P & Q>` is stored as `bounds` next to `generics` in `FuncSig` (the owner type's bounds first), and `unsatisfied_bound` checks every inferred or explicit type argument: a concrete type must conform, a type parameter of the caller must carry the protocol in its own bounds. A generic type checks the bounds of its arguments when they are concrete, and `mono` re-checks everything when it substitutes. In a generic body, a method call on a `Ty::Param` receiver and the operators `==`, `!=`, `<`, `>`, `<=` and `>=` become `TExpr::Witness { protocol, method, self_ty, args, ret }`. After substitution, `resolve_witness` in `mono` replaces it by `Witnessed::Call` of `Type.method` (with the type arguments of a generic type), `Virtual` when a subclass overrides it, or `Operator` for the built-in types. `Equatable` is implicit for `Int`, `Bool`, `String` and enums without payloads; a struct declares it and `$equal<S>` is synthesized as an instance from the same member-by-member lowering that `==` on a struct uses. `Comparable` is implemented by `Int` only, and implies `Equatable` in a constraint.

**Consequences.** The key encodes a `Witness` by protocol, method and type, and only generic bodies contain one, so reference counting and the backend never see it. A mutating requirement is satisfied by a struct, or by a non-mutating method of a class; an enum without payloads cannot satisfy it yet because its mutation goes through a cell. A protocol is used as a constraint only: there is no existential type, no associated type, no protocol inheritance, no default implementation and no static requirement. `Equatable` cannot be declared by a generic type or a class, and a user type cannot be `Comparable`, because there is no operator overloading. The protocol declaration joins the signature digest as a whole, so editing a requirement rechecks every function.

### Existentials are class references, and a value of another type is boxed (2026-11-25)

**Problem.** `let x: P` needs one representation for a value of any conforming type, a way to call a requirement on it, and reference counting, without a second aggregate kind in the typed IR.

**Options.** A class reference where a non-class conformer is boxed in a synthesized class (reuses the object header, counting, drop and tag dispatch); only class conformers (smallest, but a struct or an enum cannot be used); an inline layout of a tag and the widest payload (keeps value semantics, but the slot typing rule forbids two class or struct conformers).

**Decision.** An existential `P` is a pointer to a class object. A class conformer is stored as it is, and any other conformer (struct, enum, `Int`, `Bool`, `String`) is wrapped in `$Box<S>`, a synthesized generic class with one member `value: S`. A requirement called on an existential becomes a tag comparison chain over the class tags of every conformer, in the style of `Virtual`: the case of a class calls its method, the case of `$Box<S>` calls `S.method` with `value`. A protocol with associated types or a requirement that mentions `Self` cannot be used as a type, nor can `Equatable` and `Comparable`, and a `mutating` requirement cannot be called on an existential, because a copy of an existential shares its box.

**Implementation.** `Signatures::existential` registers, per protocol, a pseudo class that has only the object header, so the reference counting, the optionals, the members and the casts of classes apply unchanged; `is_subclass` accepts a non-generic class that conforms to the protocol. `coerce` boxes any other non-generic conformer with `construct(box_of(S), [value])`, where `$Box` is a builtin generic class with the member `value`. A call on an existential is `TExpr::Dispatch { protocol, method, args, ret }`; `mono` replaces it by `Virtual` over every non-generic conformer (`existential_conformers`), whose last case is the default. A class case calls its own resolved method, and a boxed case calls a thunk `$box<S>.method`, built by `boxed_requirement` as an ordinary instance, that reads `value` and calls `S.method`. The box classes of every non-generic type that declares a user protocol are instantiated when the signatures are collected, so that their tags, which are structure identifiers, do not depend on which functions a build checks: a clean function keeps cached tags, and an incremental test guards this.

**Consequences.** Adding a conformer changes the key of every function that calls a requirement of that protocol through an existential, as adding an override does, while unrelated functions stay cached. `is`, `as?` and `as!` from an existential to a class reuse the tag test; a cast to a protocol is rejected. A boxed struct has reference semantics through the box. A generic type cannot be stored in an existential, because the chain cannot list the instances, and an existential does not satisfy a constraint (`T := P`) yet. Calling a requirement of a protocol that nobody conforms to is an error reported while monomorphizing, without a line number. `Rewriter::expr` keeps its big arms in separate functions: a larger frame made a 120-term expression overflow the default thread stack in a debug build.

### An associated type is an opaque projection of a type parameter, bound by typealias (2026-11-25)

**Problem.** A requirement such as `func first() -> Item` mentions a type that only the conforming type knows, and a generic body must name it (`C.Item`) without knowing it.

**Options.** Infer the binding from the method signatures (Swift; needs a matching pass over every requirement); require an explicit `typealias` in the conforming type; a new `Ty` variant for the projection versus an extra field on `Ty::Param`.

**Decision.** `associatedtype Item` in a protocol registers a global identifier (`Signatures::assoc_types`). A conforming struct, class or enum binds it with `typealias Item = Type` in its body, and `check_conformances` rejects a missing binding. `Ty::Assoc` packs the type parameter index and the identifier into one word, so that `Ty` stays two words: a third word made `TExpr` larger and the checker overflowed the default thread stack on a 120-term expression, so a unit test now guards the size. Inside a protocol, a bare name resolves to the projection of `Self`; in a signature or a body, `T.Item` is a scope entry that `scope_for` adds for every type parameter whose constraints declare `Item`, and `type_scope` does the same for the members of a generic type, projecting a concrete argument through its binding at once. The projection is opaque (copy, pass, return and store) and compares equal only to itself; `unify` ignores it and `coerce` checks it after the other parameters are inferred. `subst` replaces it by the binding of a concrete argument (`assoc_binding`, which follows the superclass chain) or by the projection of the caller's own parameter. Inside the type's own members, its aliases are in scope, so a member may write `Item` for the bound type.

**Consequences.** `mono` needs no change: types of a generic body are substituted by `subst`, and a missing binding is reported there. An associated type has no constraints of its own (`associatedtype Item: P` and `where` clauses are not supported), so its values support no operation, and a nested projection (`C.Item.Item`) is rejected. A protocol with associated types must stay a constraint, which the existential plan already assumes. The binding is part of the type item, so editing a typealias rechecks every function through the signature digest.

### fatalError is a statement that writes to standard error and traps (2026-11-25)

**Problem.** A path that must never run needs a diagnostic, and a function that returns a value has to be allowed to end in one without a dummy `return`.

**Options.** Write to standard output with `puts`; write to standard error and trap; trap silently. A `Never` type usable in expressions versus a statement.

**Decision.** `fatalError()` and `fatalError("literal")` are statements, checked like `print` (a reserved name, no value). The checker turns them into `TStmt::Fatal(text)` holding `Fatal error: <message>` and a newline. The backend flushes every C stream with `fflush(NULL)`, so output printed before the call survives, writes the text with `write(2, ...)` and traps. `always_returns` and the `guard` exit analysis treat it as the end of a path, as they treat `return`.

**Consequences.** The message must be a string literal, because a `String` has no operations yet. There is no `Never` type, so `x ?? fatalError()` and `-> Never` are rejected. Live objects are not released, so their `deinit` does not run. The process ends with a trap signal and not with a status code.

### Deep input is bounded by the parser and every pass runs on a large stack (2026-11-25)

**Problem.** The parser, the checker, `mono`, the reference counting pass, the key and the code generator recurse over the depth of an expression, so a long chain such as `1 + 1 + ... ` overflowed the default 2 MiB thread stack, and the limit depended on the frame size of each function: adding a match arm to `Rewriter::expr` made a 120-term expression crash twice.

**Options.** Swift's parser caps structure nesting at 256 (`StructureMarkerRAII`, the error `structure nesting level exceeded maximum of 256`) and parses an operator chain as a flat sequence, with only a complexity limit in the type checker; `rustc` grows the stack on demand with `stacker`; a flat representation of chains in the syntax tree and the typed IR with iterative passes.

**Decision.** Two limits in `limits.rs`, both enforced by the parser as errors with a line number. `MAX_NESTING` (256, as in Swift) bounds the recursion of the parser itself: blocks, parentheses, argument lists, type arguments, prefix operators, `??` and `?.` chains. `MAX_DEPTH` (8000) bounds the depth of the tree that the later passes walk: `Expr` carries its own depth, computed when a node is built, and the parser adds the depth of the open blocks, of an `else if` chain and of the cases of a `switch` (lowered to nested `if` statements), so a chain of operators, of member accesses or of calls is accepted up to 8000 links. `driver::build_with` runs on a thread with a `STACK_SIZE` of 256 MiB and the code generation workers use the same size. The limit was measured in a debug build, which has the largest frames: 15000 operator terms fit in 256 MiB, so 8000 leaves a margin of nearly two, and tests build every shape (operators, methods, `else if`, `switch`, nested blocks, parentheses, prefix operators) at its limit, plus a parallel build.

**Consequences.** A program beyond a limit gets an error instead of a crash. Unlike Swift, a long operator chain is limited, by 8000 terms, because the tree is deep and not flat. The 256 MiB is address space and is committed only when used. Postfix parsing now moves the expression instead of cloning it at every link, which was quadratic. A new construct that adds a recursion level or a lowering that nests statements must be counted in the parser, and the frame size of the recursive functions no longer decides the limit as long as the margin holds.

### Explicit type arguments reuse the inference path of a call (2026-11-25)

**Problem.** A type parameter that appears only in the result (`func none<T>() -> T?`) cannot be inferred, and explicit arguments were accepted only for types and initializers.

**Decision.** `f<Int>(x)` already parsed as `GenericCall`; `generic_call` now hands a name that is not a type to `call_with`, whose `explicit` path resolves the arguments, checks their number and passes them to `finish_call`, where the constraints are checked like inferred arguments. A method call `base.name<Args>(...)` is a new `ExprKind::GenericMemberCall`: the parser takes the type arguments only when `(` follows on the same line, and `a.b < c` stays a comparison. `method_call` prepends the type arguments of the receiver's type to the explicit ones, so the count that the caller writes is that of the method only, and the same path serves a call through `self` and a bare call inside a method. A requirement called through a constraint or a protocol value takes no explicit arguments.

**Consequences.** `a.b<c>(d)` is read as a generic call, as `a<b>(c)` already was. An initializer still cannot have type parameters of its own.

### A nested member assignment stores through the address of the container (2026-11-25)

**Problem.** `AssignField` names a local, so only `x.m = v` could be assigned; `c.a.b = 1` has to write into storage that is inside another value (inline in a struct) or behind a reference (a class).

**Decision.** `TStmt::AssignPath { base, index, value }` evaluates `base`, a chain of `Field` reads, to the address of the container and stores the member there. Reading a struct member already yields the address inside its parent, and a class member yields the pointer, so one chain serves both. The checker builds the chain with `assignable_path`, which accepts a root variable (or an implicit `self` member) followed by members and refuses a call result or an `unowned` or `weak` hop, because reading such a member takes a counted reference that the chain would evaluate twice. A value-type member on the path must be `var`; a class member on the path need not be, because the object it holds is mutated and not the member. The root variable must be `var` unless the path crosses a class, since then the storage being changed is the object's. The reference counting pass shares its three store routines through a `Holder` that is a local or a path, so the old value of a counted member is released, and a weak or unowned member keeps its own counting, as with `AssignField`. A one-level assignment to a local keeps `AssignField`, so existing keys do not change.

**Consequences.** The base of a path is evaluated twice (to release the old value and to store), which is correct because it consists of field reads only. A path through an optional (`a?.b = 1`) and through a call result is still rejected.

### An initializer is named after its labels and types, and may delegate to another (2026-11-25)

**Problem.** An initializer was the function `Type.init`, so a type had one. Several initializers need distinct names and a way to choose among them, and the usual reason to have several is that one calls another.

**Options.** Overload by the labels only (deterministic, no typing of the arguments before the choice) or by the labels and the types of the arguments (as Swift does); for delegation, copy the code of the other initializer or call it.

**Decision.** `FuncDecl::with_owner` names an initializer `Type.init(label:Type,...)` from its parameters (`_` for a missing label, the type as written), so two initializers with the same labels and types are a duplicate, and `Signatures` keeps the names of the initializers of a type in declaration order. A call `Type(args)` goes through `init_call`. With one initializer the old path and its messages stay. With several, the ones whose argument labels match are the candidates; their argument expressions are checked once, with an expected type only where every candidate agrees on the parameter, and `initializer_score` picks the candidate whose parameters match best (an exact or generic match counts, a conversion such as a subclass, an optional or a protocol value does not), a tie being an ambiguity error and no match listing the choices. `self.init(args)` is a statement that builds a value with `Type(args)` and replaces `self`: a struct or enum assigns it, a class moves the members of the built object into `self` and frees it without running `deinit`, as `super.init` does. `init_check` accepts a delegating initializer when `self.init` is a top-level statement, is called once and nothing before it uses `self`; after it `self` is complete. For an enum, `self.init` counts like `self = ...` on a path, so both branches of an `if` may delegate. An initializer that delegates to itself, directly, is an error.

**Consequences.** Cycles through several initializers are not detected and recurse at run time. A `nil` or other literal without a type cannot choose between candidates that disagree on that parameter. A `let` member can still be assigned after delegating. The names of initializers are part of the symbols, so a signature change renames the function.

### A string is a counted block and its operations are one typed IR node (2026-11-25)

**Problem.** A string was a pointer to static NUL-terminated bytes with no length, so no operation could create one; concatenation needs heap memory, and the memory has to be released.

**Options.** Leak what an operation allocates (the first step of classes); count references. The same choice for a payload that mixes a string with another type at one position, where one slot cannot tell the counting pass which case it holds.

**Decision.** A string value points at its first byte, with a header before it: the reference count at `-16` and the length in bytes at `-8`, and a NUL after the last byte, so `puts`, the equality loop and every earlier use of the pointer still work. A literal is read-only data with the immortal count `i64::MIN`; retaining or releasing it is skipped by a sign test. `needs_rc` counts `Ty::Str`, so the pass that handles classes handles strings: variables, parameters, results, members, payloads and optionals. A payload enum gives a string at a position that other cases fill with another type a slot of its own after the shared ones (`CaseSig::slots`), so the layout alone says which slots are counted. `TExpr::StrOp` carries the operations: concatenation and `String(Int)` allocate (`allocates` makes the pass treat the result as owned), and `count`, `isEmpty`, `hasPrefix`, `hasSuffix` and `contains` read. `String(Int)` is `snprintf` with the Apple arm64 variadic padding, `String(Bool)` and `String(enum)` are `Select` over literals, ordering is `memcmp` and then the lengths, and `contains` is `memmem` with an explicit answer for an empty needle. The lexer splits a literal at `\(` into `Token::Interp` pieces and the parser rewrites it as `text + String(code) + ...`, so the conversion rules are those of `String(x)`. `String` is `Comparable`.

**Consequences.** `count` and the searches work on UTF-8 bytes, which equals Swift for ASCII and differs for combining characters and emoji; there is no `Character` type, indexing or slicing. Ordering is by bytes, which is the order of code points. A payload enum that mixes a string with other types is slightly larger. A 3 million iteration loop of concatenations keeps the resident set at 1.4 MB. A struct, a class or an optional cannot be interpolated.

### A leading dot takes its enum from the expected type (2026-11-25)

**Problem.** `.circle(5)` has no type name, so the checker has to find the enum from the context.

**Decision.** The parser reads a leading `.name` or `.name(args)` as `ExprKind::ImplicitMember`; a statement cannot start with it, and a `.` on a new line already began a new statement. The checker uses the expected type that `nil` already uses, and so every place that sets it gives a type to a leading dot: an annotation, a `return`, an assignment, an argument whose parameter type is known, a member initializer, a payload, and now the other side of `==` and `!=` (`c == .red` and `.red == c`; the typed IR keeps the order of the operands). `enum_case` and `enum_construct` take an `EnumTarget` (definition, explicit arguments, name) instead of an expression, so the implicit form reuses them with the expected type as context, which also infers the arguments of a generic enum, and a local that shares the name of the enum does not hide it. A context `E?` looks through to `E` and wraps the result; when `E` has no such case, `.none` and `.some(x)` mean the cases of the optional.

**Consequences.** Only enum cases are implicit members: there are no static members, no `.init` and no cases of a struct. A context that is not decided before the operand (a `let` without annotation, `print(.a)`) is an error. An optional compared with an implicit case is still rejected, because equality on optionals is not supported.

### Classes dispatch and cast through a descriptor in the object header (2026-11-25)

**Problem.** The single inheritance decision compared the class tag with the tags of every subclass that existed when a function was checked, both for overridden methods and for casts. The instances of a generic class (`Derived<Int>`) are created while the bodies are checked, and an incremental build checks only the dirty functions, so the list of instances differed between builds and a cached caller could dispatch or cast with an incomplete list.

**Options.** Allow generic hierarchies without override and casts (sound, small); a descriptor in the header (exact, large); restrict the subclass to a plain class (closed world, but `Derived<T>: Base<T>` is not expressible).

**Decision.** The third header slot of an object, which held the address of the drop function, holds the address of the descriptor of its class: the drop function, the descriptor of the superclass, then one function address per virtual method. `Descriptor` (typed IR) carries the names, so the key of a function that creates or tests a class contains them. `construct` writes a `ClassDesc(ty)` placeholder and `mono` expands it in the function that creates the object (`Rewriter::describe`), once the type is concrete: it builds the chain of superclasses, resolves each virtual method to the function of the nearest class that declares it, and creates the instance of a generic method of a generic class like any other call, recording the dependency. The code generator emits a descriptor as a data blob with relocations to function and descriptor symbols (`Literal::with_relocs`, cache format 2), defined once per name, in writable data because it holds addresses. The virtual methods of a class are those of its superclasses first and its own after, each class sorted by name, so a slot is the position of the name in that list and stays valid in every subclass. A call is `TExpr::VCall { slot }`, a load of the descriptor and of the entry and an indirect call, and only when some subclass declaration overrides the method, which is read from the declarations and not from the instances; otherwise it stays a plain call. A cast is `TExpr::InstanceOf`: a loop over the descriptors of the object and its superclasses comparing with the target. `rc` adds the classes of every descriptor to those that need a drop function, because the table points at them. The class tag in the header stays for dispatch through a protocol value, whose conformers are plain classes.

**Consequences.** The key of a caller no longer contains the list of overriders, so adding a subclass recompiles no caller; it changes only when the first override makes a call virtual. Every class that is created adds one descriptor and the functions of its table to the object, even if a method is never called. A method with type parameters of its own cannot be overridden or be an override, because it is not in the table. An override must keep the signature of the method it replaces after the type arguments of the superclass are applied. A call through the table costs two loads more than a direct one and replaces a chain of comparisons, with no difference in a loop that calls two overrides (0.55 s against 0.60 s for 50 million iterations). The earlier decision's tag comparison for overridden methods and for casts is superseded; the part about layout, `super` and initializers stands.

### A payload value may carry a label, required at construction and optional in a pattern (2026-11-26)

**Problem.** The payload of a case was a list of types, so `case rect(width: Int, height: Int)` could not be declared and the call `.rect(width: 1, height: 2)` was rejected.

**Decision.** `EnumCaseDecl` and `CaseSig` keep a label (or none) per value, parsed like an argument label: a name followed by `:`. A construction goes through `expect_label`, as a call does, so a declared label must be written, and a value without a label cannot be given one; the implicit form (`.rect(width: 1, height: 2)`) and the inference of a generic enum use the same path. A pattern may bind with `case .rect(let w, let h)` or repeat the labels (`width: let w`, `width: _`); a label that it writes must be the one the case declares. The labels take no part in the layout or in the keys, because the values keep their positions.

**Consequences.** Two cases cannot be told apart by their labels, since the name of the case is what selects it. `_` as a label and a label that differs from the name of a binding (`case rect(width w: Int)`) are not supported.

### Several patterns of one case share one set of bound names (2026-11-26)

**Problem.** A case that lists several patterns could not bind payload values, because the body needs one local per name while each pattern reads it from another slot.

**Decision.** `case_patterns` returns a test and a prelude for every pattern. The first pattern declares its names; a later one must bind exactly the same names (`the patterns of a case must bind the same names`), and `bind_payload` reuses the local of a name already in the scope of the case, requiring the same type, while a name that is bound twice by one pattern is still an error. The body is checked once. If no pattern binds anything, the tests are joined with `||` into one branch as before; otherwise every pattern becomes a branch of the chain with its prelude and a copy of the body, which uses the same locals, so only one copy runs.

**Consequences.** The code of a shared body is repeated once per pattern. Patterns that bind nothing may not be mixed with patterns that bind, because the names would differ. A pattern list that mixes a case with a class payload and a case with an `Int` at the same position still falls under the layout rule for payloads.

### The deinit of a generic class is an instance created with its descriptor (2026-11-26)

**Problem.** A `deinit` of `Box<T>` is generic over the type's parameters like a method, but the drop function that calls it is built from the typed IR of a concrete class after monomorphization and never passes through it, so nothing created the instance of the `deinit` for `Box<Int>`.

**Decision.** `Signatures::deinit_of` and `deinit_name` give, for a class instance, the function `Owner.deinit` with the type arguments of that instance and the name that `mono` gives its instance (`mangled_call`, shared by `Instantiator::instance_for`, so the two cannot differ). `describe`, which already runs for every class that an object of the program belongs to, creates the instance for every class in the chain of superclasses that has one and records it as a dependency of the function that creates the object, so the instance is compiled, keyed and listed in the item memo like a method. `rc::drop_function` calls it by that name, walking from the class up through its superclasses as before. The restriction on a `deinit` in a generic class is gone.

**Consequences.** Editing the body of a `deinit` of a generic class rechecks every function, as editing any generic body does, and recompiles only its instances. A `deinit` is part of the object whenever any descriptor of the class exists, even for a class that is only cast to.

### An optional comparison is a lowering over the tag and the payload (2026-11-26)

**Problem.** Equality was supported for scalars, strings and structs, but `a == b` on `T?` was rejected: an optional is a payload enum, and a payload enum has no equality.

**Decision.** `==` and `!=` with an optional on either side coerce both operands to that optional type (a plain value is wrapped, so `a == 5` and `5 == a` work, and so does `c == .red`) and `optional_equality` lowers the comparison: both operands are bound once, left before right, and the result is `tags are equal && (tag is none || payload equal)`. The payload goes through `equality_of`, which picks the comparison of the payload type: the operator for `Int`, `Bool`, `String` and enums without payloads, `struct_equality` for a struct, `optional_equality` again for a nested optional, and a `Witness` for a type parameter that is `Equatable`, so `Optional<T>` works in a generic body. `struct_equality` compares its leaves with `equality_of`, and `equality_leaves` accepts an optional leaf whose wrapped type is comparable, so an `Equatable` struct may have optional members. `conforms` makes `Optional<T>` `Equatable` when `T` is, and the instance that `mono` builds for it (`equality_function`) uses the same lowering. `.some` and `.none` of an optional context now mean the cases of the optional itself, even when the wrapped type is an optional.

**Consequences.** An optional of a class, of a struct with a class member, or of a payload enum cannot be compared, as the wrapped type cannot. The comparison of an optional string allocates nothing and keeps the resident set flat in a loop of 2 million comparisons.

### A Character is a Unicode scalar and the character operations are string operations over UTF-8 (2026-11-26)

**Problem.** A string had no operation on its characters, and `count` is the number of bytes. The language has no collection, so an operation cannot return an array, and a grapheme cluster, the unit of Swift's `Character`, needs Unicode tables.

**Decision.** `Ty::Char` is a 64-bit scalar value (`TExpr::Char` is its literal). A string literal whose expected type is `Character` (or an optional of it) is checked to hold exactly one scalar, and the expected type now also reaches the other operand of a comparison when it is a string literal, so `c == "a"` and `"a" < c` work; a `switch` on a `Character` takes literals and ranges (`"a"..."z"`), `Character` is `Equatable` and `Comparable`, and `String(c)`, an interpolation and `print(c)` encode it as UTF-8 (`StrOp::FromChar`). `Character(65)` and `Character("w")` check that the value is a scalar (not above U+10FFFF, not a surrogate) or that the string holds exactly one, and trap otherwise. `c.isLetter`, `isNumber`, `isWhitespace`, `isUppercase`, `isLowercase`, `isPunctuation`, `isASCII` are tests over ASCII ranges and `c.unicodeValue` is the value; a scalar above 127 is none of those. A `for c in s` lowers to a loop over a byte offset that decodes the scalar at the offset (`CharAt`) and advances by its width (`CharWidth`) before the body, so `continue` still advances. The members of `String` are `StrOp` nodes, which allocate when they build a string: `first` and `last` are optionals built with `Select`, `reversed` copies scalar by scalar, `uppercased` and `lowercased` change ASCII letters only, `prefix(n)`, `suffix(n)`, `dropFirst(n)` and `dropLast(n)` count `n` scalars from the start or the end (a negative `n` traps, a count above the length takes everything), `replacingOccurrences(of:with:)` replaces non-overlapping matches from the left (an empty target changes nothing), `trimmed()` removes ASCII whitespace at both ends, and `firstIndex(of:)` is the offset in scalars of the first match (an `Int?`, not a `String.Index`). `contains` takes a `Character` too.

**Consequences.** `count` stays the number of bytes while `prefix`, `suffix`, `dropFirst`, `dropLast`, `firstIndex` and the loop count scalars, so `s.prefix(n).count` can exceed `n`. A grapheme cluster (an emoji with a modifier, a letter with a combining mark) is several `Character` values. Names that Swift spells differently are `trimmed()` (Swift trims with a character set) and the `Int` result of `firstIndex(of:)`. `split`, `joined` and the subscripts are described in a later decision. A loop of a million iterations that trims, reverses, changes the case, replaces and searches a string keeps the resident set at 1.5 MB.

### An array is a builtin generic class with a separate buffer and copy on write (2026-11-26)

**Problem.** The language had no collection. An array needs value semantics (`let b = a; a.append(1)` must not change `b`), a lifetime, elements of any type that fits, and a place in the function cache, without a second aggregate kind in the typed IR.

**Options.** For semantics: reference semantics as a plain class (smallest, but unlike Swift); a copy at every assignment and call (simple, O(n) everywhere); copy on write, deciding at the write from the reference count. For storage: elements inline in the object (one allocation, but a growing array would change its address); a separate buffer behind a fixed-size object.

**Decision.** `Array<Element>` is registered after `$Box` as a generic class with the members `count`, `$capacity` and `$buffer` (all `Int`), so `[T]` is parsed to `Array<T>` and everything that a class has (reference counting, the descriptor and drop function, monomorphization, unification of `[T]` against `[Int]`) applies unchanged. The buffer is a `malloc` block of 8-byte elements. An element must fit one slot (`Int`, `Bool`, `Character`, `String`, a class, an enum without payloads, an array, a type parameter); structs, optionals and enums with payloads are rejected where `[S]` is formed, with a message. `TExpr::ArrayOp` has four operations: `Buffer` builds the buffer of a literal from owned elements, `Get` checks the index (one unsigned comparison, a trap) and reads, and `Append` and `Set` return an owned array. They share `unique_array`: when the strong count is 1 the object is reused (the buffer grows with `realloc` when needed) and retained once, otherwise a new object is built, its elements are retained one by one, and the old reference stays with its holder. The checker lowers an update to an assignment of the result to the same place (`a = Append(a, x)`, `s.items = Set(s.items, i, v)`), through the existing assignment of variables, members and nested members, so the old reference is released by the assignment and the count is 1 again afterwards. `a[i] += 1` binds the index once in the `Set` and reads the old element with `Get` on the same place. `for x in a` binds the array and a position, so a write to the array inside the loop copies it and the loop sees the old contents. `rc::drop_function` adds `DropArray`, which releases every element that is counted and frees the buffer. `rc` treats `Buffer` operands and the stored value as consumed, an index and the array as borrowed, and the result of `Append` and `Set` as owned; the key encodes the operation, the element type and the result type.

**Consequences.** A copy happens only when a second holder exists, so a loop that appends to a variable is amortized O(1) per element (capacity at least 4, then doubling) and a copy costs one retain per counted element. Reading `a[i]` of a counted element retains it where it is consumed, as a member read does. The array object is 56 bytes (the four header slots and three members). Two holders of one array are never visible to the program, because every write goes through the unique check. A loop that appends to the array it iterates copies it once. `a[i][j] = x` is rejected because an element of an array is not a place yet; a struct member that holds an array can be updated (`bag.items.append(1)`), as can a class member. `[]` needs a type from its context. There is no `==`, `+`, slicing, `removeLast` or `insert` yet, and the whole `String` side (`split`, subscripts) still waits.

### A multi-slot array element is stored inline with a stride (2026-11-26)

**Problem.** Arrays accepted only one-slot elements, so `[Int?]`, `[Point]` and `[Shape]` were rejected.

**Options.** Box every struct element in the heap (one slot, but an allocation and an indirection per element, and a second kind of ownership); store it inline in the buffer with a stride equal to its slot count, the way a struct member is stored in its parent.

**Decision.** The buffer stride is the number of flattened slots of the element (8 bytes for a one-slot element). A struct value is an address, so `Get` returns the address inside the buffer, `Buffer`, `Append` and `Set` copy the slots in with `copy_struct`, `unique_array` copies and retains an element through its counted slots, and `DropArray` releases the counted slots of every element. `Append` and `Set` first copy a struct value to a stack slot, because the value may point into the buffer that `realloc` or the unique copy is about to move (`a.append(a[0])`). `Set` releases the old element before it overwrites it. The rule that rejected these types in `instantiate_struct` is gone; only `Void` is refused.

**Consequences.** An array of optionals costs two slots per element and an array of a struct as many as the struct has slots. Reading an element of a struct type yields an address into the buffer, as reading a member does, so a binding copies it and the address is never kept across a write. Nothing in the typed IR, the key or the reference counting changed beyond the element type in the key.

### Array operations: codegen for the structural ones, synthesized functions for the equality ones (2026-11-26)

**Problem.** `removeLast`, `insert`, `+`, `first`, `last`, `contains` and `==` had to fit the copy-on-write model, and the last two need the equality of an element of any type inside a loop that an expression cannot contain.

**Options.** For equality: emit a comparison loop at every use site (large, and recursive for `[[T]]`); call a synthesized function per array type, as `Equatable` on structs already does with `$equal`. For `removeLast`, which both changes the array and yields a value: a statement only, as with `append`; an expression that also assigns (needs statements inside expressions).

**Decision.** `Insert`, `RemoveLast` and `Concat` join the `ArrayOp` set. `Insert` checks `0 <= index <= count`, makes the array unique with room for one more, `memmove`s the tail by one stride and stores the value; `RemoveLast` checks for an empty array, makes it unique, decrements the count and releases the removed element; `Concat` builds a new object and retains the elements of both sides (`copy_elements`, shared with the unique copy). The first two are updates, lowered like `append` to an assignment, so they work on variables and members; `+` is an expression and `+=` an update. `first` and `last` are `Select` over the count that yield an optional of the element. `a == b` and `contains` call the synthesized functions `$equal<[T]>` and `$contains<[T]>`: `equality_function` for an array compares the counts and then each pair through `equality_of`, which makes `[[T]]`, `[Int?]` and `[P]` work, and `contains_function` loops with `equality_of` against the wanted value. Both go through `mono::instance_for`, so a generic body that calls them with a type parameter gets one instance per element type. `Array<E>` conforms to `Equatable` when `E` does. An element that is not equatable is rejected where `==` or `contains` is written, by a trial `equality_of`.

**Consequences.** The result of `removeLast` cannot be used (`let x = a.removeLast()` is rejected with a message that points to the statement form), so a program reads `last` first. An empty literal on the right of `+`, `==` or `!=` takes the type of the left side. A class element is not equatable, so `contains` on `[C]` is rejected. A struct member of an array type is still not accepted in an `Equatable` struct.

### An element of an array is a place that makes its container unique first (2026-11-26)

**Problem.** `g[0][1] = x` changes the array held by an element of `g`. Computing the inner result first (`g = Set(g, 0, Set(g[0], 1, x))`) is wrong for copy on write: when `g` is shared, the inner array is held once by the shared buffer, so it would be changed in place and the other holder of `g` would see it.

**Options.** Copy the whole outer array and every inner array on any nested write (always correct, quadratic for a loop); make each container on the path unique from the outside in, then update the innermost array, so that the unique copy of an outer array retains its inner arrays and the inner write sees a count of 2.

**Decision.** `update_stmts` handles a target `base[index]` by emitting the updates of `base` itself: first `Unique`, which makes `base` unique (`ArrayOp::Unique`, the same `unique_array` that `append` uses) and recurses outwards, then `Within { index, inner }`, which is `base = Set(base, index, <inner update applied to Get(base, index)>)`. The inner update is any update (`Element`, `Append`, `Insert`, `Remove`, `RemoveLast`, or another `Within`), so `g[0][1] = x`, `g[0].append(x)` and `cube[0][0][0] = x` share one rule, and the root can be a variable, a member of a struct or a class, or an implicit member. The statements are wrapped in a block that is always entered, and `rc` treats `Unique` as owned like the other operations.

**Consequences.** After the unique pass the outer buffer is held once, so an inner array that is also held elsewhere has a count of 2 and is copied, and one that is held only by the buffer is changed in place and stored back without a copy. A nested write on a shared outer array copies that array once and retains its elements. An index expression is evaluated once per level of the path and once more for each outer level's unique pass, so an index with a side effect runs several times. A struct element cannot be changed through a member (`ps[0].x = 1`) because the member path stops at an array element.

### An expression may run statements after its value is read (2026-11-26)

**Problem.** `let x = a.removeLast()`, `a.remove(at: i)` and `while let x = a.popLast()` both yield a value and change the array, and the array is changed by assigning a new object to its place. Hoisting the assignment before the statement would be wrong for a loop condition, and an expression could not contain a statement.

**Options.** Hoist the update before the enclosing statement (breaks `while let` and short-circuit operators); a place reference passed to the operation (a local lives in an SSA variable and has no address); add `TExpr::Then { value, after }`, which evaluates `value` and then runs the statements `after`, in the position where the expression stands.

**Decision.** `Then` is the new typed IR node. The checker builds `removeLast()` as the element at `count - 1`, `remove(at:)` as the element at the index, and `popLast()` as the `Select` of nil or the last element, each followed by the existing update statements of the same place (`update_stmts`, so a variable, a member, a nested element and an implicit member all work, with the same mutability checks). `popLast` guards the update with `count != 0`, read again after the value. In `rc`, the value of a `Then` is made owned (a `Retain` when it was borrowed) and the `after` statements go through the ordinary block rewriting, so the removed element is released by the update while the result keeps its own reference. The code generator evaluates the value, copies a struct value to a stack slot, because the update may move the buffer under the address, and then runs the statements. `removeAll()` is an update that assigns an empty array. The key encodes `Then` with its statements.

**Consequences.** The result of the three members can be bound, passed, combined (`a.removeLast() + a.removeLast()` removes two elements in order) and used as the subject of `while let` and `if let`. A place is read once for the value and once for the update, so an index with a side effect in the place or in `remove(at:)` runs twice. `Then` makes the enum cell restriction (a mutating call that produces a value) removable the same way, which is left for later.

### A member of a struct element is written through the element's address after the array is made unique (2026-11-26)

**Problem.** `ps[0].x = 1` changes a struct that lives inline in the buffer of `ps`; the array has value semantics, so the write must not be seen through another holder of the same buffer.

**Options.** Rebuild the element (`ps = Set(ps, 0, copy of ps[0] with x changed)`, needs a struct update expression that does not exist and copies every slot); make the array unique and write the member through the address that `Get` already yields for a struct element, with the nested member assignment that stores through the address of a container (`AssignPath`).

**Decision.** `assignable_path` accepts a subscript as the root of a member path: its value is `Get(array, index)`, which is the address of the element in the buffer (bounds-checked), and it counts as crossing a class when the element is a class, so a member of a class element needs no unique array. `update_stmts` adds `element_prefix` before a member update: when the member path starts at an element of a struct type, the array that holds it is made unique with the `Unique` update of the previous decision, at every level of nesting, and then `assign_nested` stores through the address. The updates that take a place (`append`, `insert`, `removeLast`, element assignment, `+=`) therefore work on members of elements as well.

**Consequences.** A write to a member of a shared array copies it once (retaining the elements) and then writes in place; an unshared array is written in place. The array of a constant (`let ps`) is rejected for struct elements and accepted for class elements, as in Swift. The index expression is read once for the unique pass and once for the write, as for the other element paths. A mutating method called on an element (`ps[0].move()`) is not supported yet.

### split, joined and subscripts are synthesized functions and four byte-level string operations (2026-11-26)

**Problem.** Arrays make `split` and `joined` possible, but building an array of strings needs a loop, which an expression cannot contain, and Swift subscripts a string with `String.Index`, which this compiler replaced by an `Int` scalar offset in `firstIndex(of:)`.

**Options.** Build the array in the code generator (a large routine that knows the array layout); synthesize typed IR functions per use, as for `$equal` and `$contains`, over a few new string operations. For subscripts: a `String.Index` type (large); an `Int` offset in scalars, as `firstIndex(of:)` returns, with `s[a..<b]` taking scalar offsets.

**Decision.** Four `StrOp`s were added: `ByteIndexFrom` (the first match of a needle from a byte offset, or -1), `ByteSlice` (the bytes between two offsets, trapping unless `0 <= start <= end <= length`), `ScalarAt` (the scalar at an offset in scalars, trapping outside the string) and `ScalarSlice` (the scalars of `[low, high)`, trapping unless `0 <= low <= high <= scalar count`). `split_function` and `joined_function` build `$split` and `$joined` as typed IR with loops over these operations; a call carries `String` as its only type argument so that `mono::instance_for` creates the instance, as for `$equal`. `s.split(separator:)` takes a `String` or a `Character`, omits empty pieces, and with an empty separator returns the whole text as one piece (none for an empty text). `a.joined(separator:)` needs `[String]`, and the separator defaults to the empty string. The checker routes a subscript on a `String` to `ScalarAt` for an `Int` and to `ScalarSlice` for `a..<b` and `a...b`, whose result is a new `String` and not a `Substring`.

**Consequences.** A subscript costs a scan from the start, linear in the offset, so a loop over `s[i]` is quadratic: use `for c in s`. The result of a slice is a copy. `split` has no `maxSplits`, no `omittingEmptySubsequences: false`, and splits only on a separator, not on a predicate; `joined` is defined only for `[String]`. A string subscript cannot be assigned. Offsets count scalars, so a string with a combining mark counts the mark as an element.

### A dictionary and a set are one insertion-ordered table class, written in code generation (2026-11-26)

**Problem.** `Dictionary` and `Set` need hashing, equality, growth, removal, value semantics and a deterministic iteration order, for keys of any comparable type, and the cache and the tests need the same output on every run.

**Options.** For the order: insertion order (deterministic, a compacting rebuild after removals) or hash order (smallest, but it changes with the hash function and the capacity). For the structure: a struct of arrays composed in the typed IR (no new backend code, but every operation is IR and slow to emit), or a table class in code generation. The choice was insertion order and code generation, with the key semantics kept out of the generator: hashing and equality of a struct, optional or array key are typed IR functions, as `$equal` already is.

**Decision.** `Dictionary<Key, Value>` and `Set<Element>` are builtin generic classes registered after `Array`, with the members `count`, `$used`, `$capacity`, `$entries` and `$table`, so reference counting, the descriptor and drop function, monomorphization and unification apply unchanged. The entries live in a buffer of `capacity` records of `1 + key slots + value slots` words (a hash, the key, the value; a `Set` has no value), in insertion order; a removed record keeps its place with the hash `-1`. `$table` is an open-addressing index of `2 * capacity` words (capacity is a power of two, at least 4) holding the record index or `-1`, with linear probing, so the table is at most half full. Lookup compares the stored non-negative hash and then the key. `DictOp` has `Find`, `Set`, `Remove`, `Key`, `Value` and `Live`. `Set` and `Remove` make the table unique first (`unique_table`: reuse when the count is 1 and the records fit, rebuild in place with a compaction when they do not, copy with a retain of every live key and value when it is shared), then probe, replace the value or append a record, or mark the record removed. `DropTable` releases every live key and value and frees both buffers. A key of `Int`, `Bool`, `Character`, an enum or `String` is hashed and compared inline (a scrambling multiply, FNV-1a over the bytes); any other key goes through the synthesized `$hash<K>` and `$equal<K>`, which `mono` instantiates from the key type found in the operation (`hash_function` mixes the leaves of a struct, the tag and payload of an optional, and the count and elements of an array with `HashMix`). `keys` and `values` are the synthesized `$keys` and `$values`. The checker lowers a read to `Select` over `Find` and `Value`, a write to an update of the place (`Set`, or `Remove` for `nil`), `d[k, default: x] += 1` to a `Set` of the default or stored value combined with the right side (the key is bound once), `for (k, v) in d` to a loop over the records that skips the removed ones, and `removeValue` and `updateValue` to a `Then` over the lookup and the update. A literal builds an empty table (all five members zero, so no buffer exists until the first insertion) and a chain of `Set`s; a `Set` literal needs a `Set<E>` annotation.

**Consequences.** Lookup, insertion and removal are O(1) on average; a removal leaves a record that the next growth drops; iteration is in insertion order, and replacing a value keeps the key's place. A copy is made only at the first write to a shared table and retains every live key and value. A key must be hashable: `Int`, `Bool`, `Character`, `String`, enums without payloads, structs whose members are such values, optionals and arrays of these; a class is not. Iteration order differs from Swift's, which is unspecified.

### A dictionary entry reached through a default is a place that is ensured, then stored back (2026-11-26)

**Problem.** `d[k, default: []].append(x)` changes the array held by an entry that may not exist yet, and the array has value semantics: reading the entry (`Select` over `Find` and `Value`) retains it, so the append would see a count of 2 and copy the whole array on every call.

**Options.** Treat the entry as an optional that is read, changed and written back as a value (a copy per update, quadratic for a loop); insert the default first, then change the stored array while it is still held only by its entry, and store the result back.

**Decision.** Two operations were added to `DictOp`. `Ensure(dict, key, default)` makes the table unique, as `Set` does, and inserts the default only when the key is absent; when it is present the incoming key and default are released. It returns the owned table. `Store(dict, index, value)` releases the value at a record, moves the new owned value in and returns the table retained, without a unique check, because it only follows `Ensure`. A target `base[key, default: x]` of an update becomes the update `Default { key, default, inner }` of `base`: `base = Store(Bind(held, Ensure(base, Bind(k, key), default)), Bind(at, Find(held, k)), <inner update applied to Value(held, at)>)`, where the inner update is any update of the stored value, including another `Default` and an element assignment. The value that the inner update reads is the borrowed value of the record, so an array held only by its entry has a count of 1 and is changed in place, and one that is shared (the table was copied, which retains every value) has a count of 2 and is copied once. The key is bound once and evaluated before the default and the inner update.

**Consequences.** A loop that appends to entries is amortized O(1) per element (100000 appends keep their results). A write to a shared dictionary copies the table once at `Ensure`. A struct value reached through a default (`d[k, default: P()].x = 1`) and an optional chain (`d[k]?.append(x)`) are not supported. `Store` assumes that `Ensure` ran on the same table just before it; it is not a general operation.

### Table equality is a synthesized function that looks every record up in the other table (2026-11-26)

**Problem.** `==` on a dictionary or set has to ignore the insertion order, which differs between two tables that hold the same entries, and it has to compare values of any equatable type.

**Options.** Compare the records pairwise in order (wrong for two tables built in different orders); sort both (no ordering exists); look each live record of the left table up in the right one after comparing the counts.

**Decision.** `equality_function` handles a table type: it returns false when the counts differ, then walks the records of the left table and, for each live one, finds its key in the right table with `Find` (hashing and comparing keys as every lookup does) and returns false when it is missing or, for a dictionary, when `equality_of` says that the two values differ. Equal counts and every left key present on the right make the two key sets equal. The checker routes `==` and `!=` through the existing `$equal<T>` call, so nested collections (`[[String: Int]]`, `[Int: [Int]]`), struct keys and values, and generic code with an `Equatable` bound reuse it, and a trial `equality_of` on the value type rejects a dictionary whose values cannot be compared. `Dictionary<K, V>` conforms to `Equatable` when `V` does and `Set<E>` always does, because a key is hashable only if it can be compared. An empty literal on the right takes the type of the left side.

**Consequences.** Equality costs one lookup per entry, O(n) on average. A dictionary of class values has no `==`. A struct member of a dictionary type is still not accepted in an `Equatable` struct.

### Set algebra is a family of synthesized functions over the records of two tables (2026-11-26)

**Problem.** `union`, `intersection`, `subtracting`, `symmetricDifference`, `isSubset(of:)`, `isSuperset(of:)` and `isDisjoint(with:)` each need a loop over one or both sets and a lookup in the other.

**Options.** Emit each as a code generator routine (seven routines that all repeat the table walk); build them as typed IR functions from `Live`, `Key` and `Find`, as `$equal` for tables is built.

**Decision.** `set_function` builds `$union`, `$intersection`, `$subtracting`, `$symmetric`, `$subset` and `$disjoint` as typed IR, instantiated per set type by `mono::instance_for` (`SetOp::from_callee` selects the body). `union` starts from the left set and adds the live keys of the right one, so the first insertion copies the table because it is shared; `intersection` and `subtracting` start from an empty set and add the live keys of the left one that are present, or absent, in the right one; `symmetricDifference` adds the keys of each side that the other lacks; `isSubset` compares the counts and then looks every left key up in the right set; `isDisjoint` returns false at the first common key; `isSuperset(of:)` is `isSubset` with the arguments swapped. The result keeps the order of the left set, then the right one. The argument of an operation takes the type of the receiver, so an array literal is read as a set (`a.union([4, 5])`).

**Consequences.** Each operation is O(n) on average in the size of the walked set. The results are new sets and the operands are not changed. The mutating forms (`formUnion`, `formIntersection`, `subtract`, `formSymmetricDifference`), `isStrictSubset` and `isStrictSuperset` are not provided, and the operations apply to `Set` only, not to the keys of a dictionary.

### Collection initializers are named calls of three builtin types and a type written as a literal (2026-11-26)

**Problem.** `[Int]()`, `[String: Int]()`, `Set<Int>()`, `Set(array)` and `[Int](repeating: 0, count: n)` were rejected: a bracket form is a literal and not a callee, and the names `Array`, `Set` and `Dictionary` resolved to the builtin classes, whose memberwise initializer would have exposed the hidden members.

**Options.** Add initializers to the builtin class declarations (they would have to be written in the language and would still show the hidden members); intercept the three names in the checker and build each result from the existing operations.

**Decision.** A call whose name is `Array`, `Set` or `Dictionary`, with or without explicit type arguments, goes to `collection_init` before any user type is looked up. In the parser, a collection literal that is directly called (`[T]()`, `[K: V]()`) is read as a type when its single element is a type name, a generic name or another such literal (`expr_as_type`), and becomes `ExprKind::TypeCall`, which `collection_init` receives too. With no arguments the result is an empty table or array and its type comes from the explicit arguments or from the expected type (`let s: Set<Int> = Set()`). `Set(x)` builds a set from an array with the synthesized `$toset` (a loop of `Set` operations), accepts a set as it is, and infers the element type from the argument; `Array(x)` accepts an array as it is and a set through `$keys`; `Array(repeating:count:)` calls the synthesized `$repeating`, which traps on a negative count and appends the value `count` times, so each element is an independent value under copy on write. An element type that cannot be a key is rejected where the set is formed.

**Consequences.** The three names cannot be redefined by a program. `Dictionary` takes no arguments yet (no `uniqueKeysWithValues:` or `grouping:`), and `Set` and `Array` take one sequence that is an array, a set or a literal. `Array(repeating:count:)` runs a loop of appends, O(count). A type written as an optional or a nested generic inside the brackets of an initializer (`[Int?]()`) is not parsed, because `?` does not continue an expression.

### A closure is a lifted function and an environment on the stack, with variables captured in cells (2026-11-26)

**Problem.** `map`, `filter`, `sorted(by:)` and `Dictionary(grouping:by:)` need closures, closures see and change the variables around them, and locals live in SSA variables that have no address.

**Options.** For lifetime: escaping closures (a counted heap environment, new reference counting rules, much larger) or non-escaping only (a function type is a parameter type, the environment lives in the caller's frame). For capture: by value (a copy, but `var n = 0; xs.forEach { n += $0 }` would not work) or by reference. For calls: specialize every function per closure (no indirect calls, but a new kind of generic argument) or one indirect call through a pointer.

**Decision.** `Ty::Func` is an interned signature (parameters and result, substituted and unified like a generic instance), written `(A, B) -> R` in a type and allowed only as the type of a parameter, of a local `let` and of a local function; a function type in a field, a result, a generic argument or an array element is rejected, which is what keeps every closure from outliving the frame that made it. A closure expression or a local function is checked in a child `FnCx` and lifted to the function `Owner.$closureN` (numbered in source order, so it is stable) whose first parameter is the environment. A free name is found by scanning the body (`free::names_in`); every such name that is visible in the enclosing function (and `self`) is captured. A captured variable is moved into a one-slot cell (`Signatures::cell_of`, a layout struct), by the pass `capture::lower` over the typed IR: the variable of the enclosing function becomes a cell (a parameter is copied into a cell at entry), its reads become `Field` of the cell, and its writes `AssignPath`; inside the closure the variable is `EnvCell`, the address of the same cell loaded from the environment. The environment is a stack slot that holds the code address and the cell addresses (`TExpr::Closure`), a call is an indirect call that passes the environment first (`TExpr::CallValue`), and a function value is just that environment pointer, so it is passed and bound like an `Int`. The lifted function is registered in the signatures and instantiated through `mono::instance_for` like a generic function, with the enclosing generics as its type arguments (a dummy argument when there are none), so it is keyed, cached and listed in the item memo with the function that holds it. The arguments of a call that are closure literals are checked after the others: the parameter types come from what the other arguments fixed, and the result type is inferred from a single-expression body (a longer body needs `-> R`). Higher-order members are typed IR functions built per array and closure type, as `$equal` is: `$map`, `$filter`, `$reduce`, `$forEach`, `$containsWhere`, `$firstWhere`, `$firstIndexWhere`, `$allSatisfy`, `$sortedBy` and `$sorted` for arrays (the sort is a stable bottom-up merge sort that uses `<` or the closure), and `$mapValues`, `$filterEntries`, `$forEachEntry`, `$containsEntry`, `$allEntries` and `$grouping` for dictionaries; a `Set` converts to its keys first, and `filter` converts back.

**Consequences.** A closure costs a stack slot and one indirect call; capturing a variable costs a cell for that variable in the enclosing function, even when it is only read. A function value cannot be stored, returned or kept, so no closure can outlive its environment, and no reference counting of environments exists. Capture is by name, so a name that the closure also declares for itself is captured needlessly. A multi-statement closure whose result type cannot be taken from the callee needs `-> R`. A dictionary closure takes the key and the value as two parameters (`{ k, v in ... }` or `$0, $1`) where Swift passes one tuple. An operator is a function value only where a closure is expected. A local function may call itself (see the later decisions for generic local functions and for calls to functions declared after it).

### The remaining closure members reuse the same functions, and reduce(into:) copies its accumulator (2026-11-26)

**Problem.** `compactMap`, `flatMap`, `min(by:)`, `max(by:)`, `count(where:)`, `sort(by:)` and `reduce(into:)` were missing; `reduce(into:)` changes its first parameter, which the language has no `inout` for.

**Decision.** `HofOp` gained `CompactMap`, `FlatMap`, `CountWhere`, `MinBy`, `MaxBy`, `Min` and `Max`, built like the other members as typed IR functions per array and closure type: `$compactMap` keeps the payload of the non-nil results, `$flatMap` appends the elements of each returned array (two nested loops, so no intermediate concatenation), `$countWhere` counts, and `$minBy` and `$maxBy` scan once with the first element as the start (`min()` and `max()` use `<` for `Int`, `Character` and `String`; `max(by:)` keeps the last of equal elements as Swift does). `sort` and `sort(by:)` are updates of the place, lowered like `append`: `a = $sorted(a)` or `$sortedBy`, so a copy is untouched. `reduce(into:)` is the existing `$reduce` with a closure whose first parameter is rebound to a mutable local initialized from the parameter, returned at the end (`closure_into`), so `acc.append(x)`, `acc[k, default: 0] += 1` and `$0 += $1` change that local and the new value becomes the next accumulator. A set gets the non-mutating members through its keys.

**Consequences.** `reduce(into:)` needs a closure literal, not a function value. A closure for `compactMap` must return an optional and one for `flatMap` an array, which are the only forms checked. `sort` is a statement on a variable, a member or an element, as `append` is.

### An operator in an argument position is the closure that applies it (2026-11-26)

**Problem.** `reduce(0, +)` and `sorted(by: <)` pass an operator as a value, and an operator has no type of its own: `+` is an addition of integers or a concatenation of strings or arrays, `<` an order on three types.

**Options.** Give each operator a family of function values with overloads (a type-directed lookup that the checker does not have); desugar the operator to a closure at parse time and let the closure's checker pick the meaning from the parameter types it receives.

**Decision.** In expression position, a binary operator token that is directly followed by `)` or `,` (which no operand can be) is parsed as the closure `{ $0 op $1 }` for `+ - * / % < > <= >= == != && ||`; the unary reading of `-` is skipped in that position. The closure is then checked like any other: its parameter types come from the expected function type, so `+` adds integers in `reduce(0, +)` and concatenates strings in `reduce("", +)`, and a type error is reported as for the written closure.

**Consequences.** An operator reference works wherever a closure literal does, which is wherever a function type is expected; `let f = +` has no expected type and is rejected as an expression. The operators `===`, `!==`, `??` and the prefix operators are not covered. A wrong arity shows as a missing `$1`, as for `{ $0 + $1 }`.

### An escaping closure is a counted heap environment over counted heap cells (2026-11-26)

**Problem.** A function value could only be passed down: its environment lived in the caller's frame, so a function type was refused in a result, a field, an array or a generic argument.

**Options.** Keep two representations, the stack environment for a closure that is only passed down and a counted heap object for one that escapes (fast, but needs an escape analysis and two kinds of cell); make every environment and every cell a counted heap object (one representation, one `malloc` more per closure and per captured variable).

**Decision.** Every closure is heap-allocated. A captured variable lives in a cell, now the builtin generic class `$Cell<T>` (a class whose only member is `value`, registered after `Set` and instantiated by `Signatures::cell_of`), so the reference counting, the drop function, the descriptor and monomorphization of a class apply to it and a cell of a type parameter is substituted with the function that holds it. `capture::lower` makes a captured local of the enclosing function a reference to its cell (`Field` of the cell for a read, `AssignField` or `AssignPath` for a write, a parameter is copied into a cell at entry), and inside the closure a captured variable is the cell reference loaded from the environment (`EnvCell`). The environment (`TExpr::Closure`) is a class-shaped object: the four header slots (a count of 1 and a descriptor), the code address, the number of captured cells, and the cells themselves, each retained by the environment. Its descriptor names a drop function `$envdrop` that is instantiated per number of captured cells by `mono::instance_for` when the closure is rewritten: it releases every cell and frees the environment. `Ty::Func` is a counted pointer, so it is retained and released everywhere a class or a string is (locals, struct and enum slots, array elements, dictionary entries, parameters and results, the drop of a containing object), a closure expression is an owned value, and an indirect call reads the code address from slot `HEADER_SLOTS`. The restrictions of the earlier decision are gone: a function type may be a field, a result, an element or a generic argument. A function value is called by its name, by a field (`button.action(4)`) or where it is produced (`adder(1)(2)`, `steps[0](3)`, `ExprKind::Apply`), and `closure_value` looks through an optional expected type.

**Consequences.** Creating a closure costs one `malloc` for its environment and one for each captured variable, and a variable is a cell for the whole function that declares it; two million closures that are created and dropped in a loop keep the resident set at 1.5 MB. A closure that captures the variable that holds it, or two closures that capture each other, is a reference cycle that leaks, as every cycle of strong references does; there is no `weak` or `unowned` capture list yet. A closure keeps what it captured alive until its last reference is released, so a captured instance runs its `deinit` then. The per-call-site stack slot of the earlier design is gone, and the non-escaping uses (`map`, `filter`, `sorted(by:)`) pay for the heap environment as well.

### The mutating set operations assign the result of the non-mutating ones (2026-11-26)

**Decision.** `formUnion`, `formIntersection`, `subtract` and `formSymmetricDifference` are updates of the place, lowered like `append` and `sort`: `s = $union(s, other)`, `$intersection`, `$subtracting` and `$symmetric`, the synthesized functions of the set algebra decision, so a set that is shared with another variable is copied by the first insertion and the other one is untouched. `isStrictSubset(of:)` and `isStrictSuperset(of:)` are `$strictSubset`, which differs from `$subset` in rejecting equal counts, with the arguments swapped for the superset.

**Consequences.** The mutating forms are statements (`let t = s.subtract(x)` is rejected with the message for statements), work on a variable, a member or an element of a collection, and cost what the non-mutating form costs (a new table), not an in-place update. Their argument takes the type of the set, so an array literal is read as a set.

### A tuple is a builtin generic struct with numbered members (2026-11-26)

**Problem.** `Dictionary(uniqueKeysWithValues:)`, `zip` and `enumerated()` produce and consume pairs, and the language had no tuple type.

**Options.** A new `Ty::Tuple` with its own layout, copy, comparison and hash rules; a family of builtin generic structs, so that the layout, the copy, the reference counting of members, the equality leaves and the hashing of a struct apply unchanged.

**Decision.** `$Tuple2`, `$Tuple3` and `$Tuple4` are generic struct declarations with the members `0`, `1`, `2` and `3`, registered after `$Cell`; a tuple type `(A, B)` is parsed to `$Tuple2<A, B>` (a parenthesized list that is not followed by `->` and has two to four types), displayed as `(A, B)`, and `Signatures::tuple_of` and `tuple_elements` build and take it apart. The expression `(a, b)` is `ExprKind::Tuple` and becomes a struct initialization whose members take the expected element types; `t.0` reads the member named `0` (the parser accepts an integer after the dot), and `t.0 = x`, `swapped.0 += "y"` and `v.1 = 20` use the struct member assignment. `let (a, _, c) = t` (`StmtKind::LetTuple`) binds the value once and declares one local per name, and `for (a, b) in pairs` loops over an array of pairs as it loops over a dictionary. Because a tuple is a struct, `==` and `!=` compare its leaves, it is `Equatable` when its members are, and it is hashable, so `[(Int, Int): String]` works. `zip(a, b)`, `a.enumerated()` and `Dictionary(uniqueKeysWithValues: pairs)` are the synthesized functions `$zip`, `$enumerated` and `$fromPairs`: the first two build an array of pairs eagerly (`zip` stops at the shorter array, `enumerated` yields `(offset, element)`), and the last builds the dictionary and traps on a duplicate key as Swift does. The initializer takes anything that is an array of two-element tuples: a literal, `zip(...)`, or the result of `map { ($0, f($0)) }`.

**Consequences.** A tuple is a value like a struct: copied by member, passed and returned by address. A tuple has at most four elements. `zip` and `enumerated` allocate an array, and `zip` takes arrays only. A closure with two parameters is not applied to a tuple (`pairs.map { a, b in ... }` is not accepted), the pair is `$0` with `.0` and `.1`.

### An assignment through ?. is a guarded update of the payload in place (2026-11-26)

**Problem.** `d[k]?.count += 1`, `maybe?.x = 5` and `d[k]?.items.append(x)` assign or mutate through an optional chain, which was only accepted as an expression or as a call statement, and the entry or the payload is a value inside a table or an optional.

**Options.** Rebuild the whole value and store it back (needs a struct update expression that does not exist and copies every slot); update the payload where it lives, as `ps[0].x = 1` writes through the address of an element, behind a check that the value is present.

**Decision.** The parser accepts an optional chain on the left of an assignment and `?[` as the start of a link. `chain_update` splits the chain into its root (the subject) and the member names of the link. For a dictionary subject (`d[k]`), the guard is `Find(d, k) >= 0`, the prelude makes the table unique with `Ensure` and a zero value that is dropped because the key is present (`Update::EnsureKey`) and finds the entry again, and the root is the entry's value, which is the address of the record in the buffer for a struct and the pointer for a class. For an optional variable or member the guard is the tag of the optional, and the root is its payload slot. The member names are walked with `hop` and the last one is assigned with the existing nested member assignment (`assign_at`), so `?.count += 5`, `?.inner.count = 7`, a class member and a collection member (`?.members.append(x)`, which uses the update of the collection) all share the rules and the mutability checks of a plain nested assignment. A call of a mutating collection member directly on the entry (`d[k]?.append(x)`) is `Update::Default` without a default, inside the same guard. The whole statement is an `if` on the guard, so nothing happens and nothing is inserted when the key or the value is missing.

**Consequences.** The entry or payload is changed in place, and a table or an optional that is shared with another variable is copied first (the table by `Ensure`, the optional by the ordinary assignment path). The key expression is evaluated for the guard and again for the update. A `let` dictionary is rejected even for class values, where Swift would accept it. Calls of user methods on the payload keep the earlier behavior (a mutating call inside a chain is rejected), and chains longer than one `?.` on the left of an assignment are not supported.

### A default entry is ensured with its default, then its value is written in place (2026-11-26)

**Problem.** `d[k, default: P()].x = 1` changes a member of a value that may not be in the dictionary yet, and the default must be inserted exactly when the key is absent.

**Decision.** The update of a member whose base path ends in a subscript with a default (`default_member_update`) is the sequence of the optional-chain update without its guard: `d = Ensure(d, k, <default>)` (the existing `Update::EnsureKey`, now taking the default expression, which makes the table unique and inserts the default only when the key is missing and drops it otherwise), a lookup of the entry, and a nested member assignment through the address of the record (a struct) or the pointer (a class), walking the member names between the subscript and the target. It serves every update, so `.x += 1`, `.inner.x = 3` and `.tags.append(x)` work as well, with the mutability checks of a plain nested assignment.

**Consequences.** The default expression is evaluated once, as an argument of `Ensure`, and the key twice (the ensure and the lookup). A `let` dictionary is rejected, even for class values. A shared dictionary is copied by `Ensure` before the entry is written.

### A weak or unowned capture is a cell that holds the object weakly (2026-11-27)

**Problem.** A closure that is stored in an object and captures the object strongly is a reference cycle that leaks; Swift breaks it with a capture list (`[weak self]`, `[unowned self]`), which this language had no syntax for.

**Options.** A new kind of environment slot with its own retain, release and read code; reuse the weak and unowned members of classes, which already count unowned references, read as nil or trap when the object is gone, and release the reference when the holder is dropped, by holding the object in a cell class whose only member is weak or unowned.

**Decision.** `{ [weak self, unowned other] params in ... }` is parsed as a capture list in front of the parameters (and `[weak self] in` alone for no parameters); the list is taken only when it is followed by `in`, a parameter list or a parameter name, so `{ [1, 2].count }` stays an array literal. Two builtin generic classes were added after the tuples: `$WeakCell<T>` with `weak var value: T?` and `$UnownedCell<T>` with `unowned let value: T`. For a name listed as `weak` or `unowned`, the closure expression creates a fresh cell at the place where the closure is made (`construct` of the cell class with `Some(x)` or `x`, so the existing rules for weak and unowned members count the reference), and the environment retains that cell like any other. Inside the closure the name is declared with the type `T?` (weak) or `T` (unowned), as a constant, and `capture::lower` reads it as the field of the cell loaded from the environment, which the reference counting pass already turns into a weak read (nil when the object is gone) or an unowned read (a trap when it is gone); it reads the member of the cell class, so no new typed IR node and no new code generation were needed. The enclosing variable stays an ordinary variable and is not moved into a strong cell. A nested closure that uses the name inherits the kind and shares the parent's cell. `[self]` is accepted as the default strong capture; any other entry without `weak` or `unowned` is rejected, because a copy of a value is not supported, and `weak` or `unowned` on a value that is not a class instance is an error.

**Consequences.** `{ [weak self] in self?.x }` needs the explicit `?.` as in Swift, because `self` is optional inside the closure. A closure with a weak capture keeps the object only as long as another strong reference does, so an object that stores such a closure is freed when its last strong reference goes (a loop of one million objects that each store a closure over themselves keeps the resident set at 1.4 MB). A weak or unowned capture costs one cell per closure creation. 

### A copied capture is a private cell made when the closure is made (2026-11-27)

**Decision.** A capture list entry without `weak` or `unowned` (`[x]`) or with a value (`[x = expr]`, `[weak held = b]`) captures a copy: the closure expression evaluates the value in the enclosing function at the moment the closure is made, puts it in a fresh `$Cell<T>` that no variable shares, and the environment retains that cell, so the enclosing variable is neither moved into a cell nor seen by the closure afterwards. Inside the closure the name is a constant (assigning to it is rejected). A struct is copied by member, an array, a dictionary or a string is shared until one side changes it (copy on write), and a class instance is the same instance (the reference is copied). The entry `[self]` stays the default capture by reference. A nested closure that uses a copied name shares the parent's cell.

**Consequences.** `fs.append({ [i] in i * 10 })` in a loop captures each value of `i`. A copied capture costs one cell per closure creation. An entry that names a variable that does not exist, or an expression that does not type check, is reported where the capture list is written.

### Tuple labels are part of the type, and the other tuple forms lower to existing statements (2026-11-27)

**Problem.** Labeled tuples (`(min: Int, max: Int)`, `r.min`), destructuring assignment, tuple patterns in `switch` and the printing of a tuple were missing.

**Decision.** The labels are part of a tuple type, carried as extra generic arguments of the builtin tuple structs: `$Tuple2` and its siblings now have the generics `T0 T1 L0 L1`, the members use only the `T`s, and an `L` is a label token, a new `Ty::Tag(i)` that names an interned label (`Tag(0)` for no label). `Signatures::tuple_of_labeled`, `tuple_elements` and `tuple_labels` build and read it, the type is displayed as `(x: Int, y: Int)`, and a type written with labels is parsed to the name `$Tuple2(x,y)`, which `resolve_type` reads back. `member_of` turns a label into the member position, so `t.x` and `t.0` read the same member and every assignment path accepts a label. A labeled and an unlabeled tuple of the same element types are different types; `coerce` converts between them where each pair of labels agrees or one side has none, by retagging the value (`TExpr::Retag`, a node that changes only the static type, since the layouts are identical), and a literal takes the labels of the expected type; `==` converts its right side the same way. `enumerated()` yields `(offset:, element:)`. `(a, b) = value` (`StmtKind::Assign` with a tuple target) evaluates the value into a hidden local and assigns each element through the ordinary assignment of its target (so a variable, a member and an element all work, and `_` skips one). `switch (a, b)` binds the subject and each element once, then tests each pattern element with the existing value test (literals and ranges), binds `let` names as constants at the start of the body, and joins the alternatives of a case with `||`; a switch with no `default` must end with a case that matches every tuple. A tuple is printed and converted to a `String` as Swift prints it: `(1, "a")`, `(x: 3, y: true)`, with the elements `Int`, `Bool`, `String`, `Character`, an enum without payloads or another tuple.

**Consequences.** A labeled tuple costs nothing at run time; the label lives only in the type. A case that binds names takes one tuple pattern, as an enum case does. Tuples hold at most four elements, patterns inside patterns are not supported, and a tuple that holds a struct, a class or an array cannot be printed.

### if case is a switch with one case and a default that is not a break target (2026-11-27)

**Decision.** `if case <pattern> = <subject> { ... } else { ... }` (`StmtKind::IfCase`) takes the same pattern as a `case` of a `switch`: an enum case with bindings (`.circle(let r)`, `.some(let v)`, `.none`), a tuple pattern (`(0, let y)`), or a literal or range. The parser shares the case-label code with the `switch`. The checker lowers it through `switch_stmt` with two cases, the pattern with the `then` body and a `default` with the `else` body (empty when there is none), so the exhaustiveness, binding, tuple and value rules are the ones of the switch. The result is wrapped in an `if true` block for the scope of the subject, and it is not registered as a target of `break`: a `break` or a `continue` inside the body leaves the enclosing loop, as in Swift, and `else if case` chains. The names bound by the pattern are visible only in the `then` body.

**Consequences.** `if case` costs what a switch costs (one comparison chain). Only one pattern per statement is supported: `if case .a = x, cond`, `guard case`, `while case` and the `case let .circle(r)` spelling are not.

### Condition lists are evaluated in order and bind names for what follows (2026-11-27)

**Decision.** `if`, `while` and `guard` take a comma-separated list of conditions (`StmtKind::IfConds`, `WhileConds`, `GuardConds` with `Vec<Condition>`). A `Condition` is a Bool expression, a `let` or `var` binding of an optional, or `case <pattern> = <subject>` with the patterns of a `switch`. The conditions run left to right and stop at the first that fails; a name bound by one is visible in the later conditions and in the `then` body (for `guard`, in the rest of the scope, and the `else` body must leave the scope). A `while` evaluates the whole list again on every iteration, and a label, `break` and `continue` apply to the loop as before.

**Consequences.** A failed condition costs no more than the comparisons before it. The `case let .circle(r)` spelling, `where` clauses and nested tuple patterns are still unsupported.

### A let before a pattern applies to the bindings of its first pattern (2026-11-27)

**Decision.** `case let .circle(r)` and `case let .rect(w, h)` are read as `case .circle(let r)` and `case .rect(let w, let h)`: after `case`, an optional `let` makes every payload position of the first pattern a bare name or `_`. The parser has one `case_label` for `switch`, `if case`, `guard case` and `while case`. As in Swift, the `let` covers only the first pattern of a list; a later alternative spells its own `let` (`case let .a(r), .b(let r)`), and a `let` inside a pattern that is already under a shared `let` is an error.

**Consequences.** No checker or code generation change, since the parser produces the same `Pattern`. `case let (a, b)` for tuples and `case var` are still unsupported.

### A where clause guards a case and a failed guard falls to the next case (2026-11-27)

**Decision.** `case <pattern> where <condition>:` (`SwitchCase::guard`) is accepted on every kind of `switch`. As in Swift, the guard belongs to the last pattern of the case only: `case .a, .b(let n) where n > 5` matches `.a` unconditionally. A case with a guard never counts toward exhaustiveness for the pattern the guard belongs to, and a last tuple case with a guard is not taken as the default. On values and on tuple cases with several patterns, which bind no names, the guard is `&&`-ed into the test of the last pattern. Where names are bound (enum payloads, a tuple pattern with `let`), the checker allocates a hidden `matched` flag per switch and lowers the cases to sequential `if !matched && test { matched = true; bindings; if guard { body } else { matched = false } }` followed by `if !matched { default }`, because the guard can run only after the bindings and a failed guard must continue with the later cases. A switch without a guard keeps the `else if` chain.

**Consequences.** A guarded switch costs one flag and the tests of the cases before the match. `where` on `for`, `if case` and `guard case` is not supported (the condition lists already take `, condition`). The Bool switch error named the case that was present instead of the missing one; it is fixed.

### zip converts each sequence to an array before pairing (2026-11-27)

**Decision.** `zip(a, b)` accepts anything a `for` loop walks: an array, a `String` (elements are `Character`), a `Set` (insertion order), a `Dictionary` (elements are `(key, value)` pairs) and a range written in the argument (`zip(0..<3, xs)`, `zip(xs, 1...2)`). The checker converts each argument to an array and passes both to the existing `$zip`: a string through the synthesized `$characters`, a set through `$keys`, a dictionary through `$entries` (a mode of the table listing function that builds pairs), and a range through `$range(lo, hi)`, where an inclusive range passes `hi + 1`. The result is still an array of pairs, so `zip(...)` of any sequences can be looped over, indexed and counted.

**Consequences.** Each converted sequence costs one array allocation before `zip` allocates its own. An inclusive range ending at `Int.max` traps on the addition instead of iterating. A range with no upper bound (`zip(0..., xs)`) and a range held in a variable are not supported, since ranges are not values.

### A tuple pattern element is a pattern of its own (2026-11-27)

**Decision.** An element of a tuple pattern is `_`, `let x`, a value or range, another tuple pattern in parentheses (`TuplePart::Tuple`), or an enum pattern such as `.circle(let r)` or `Shape.square` (`TuplePart::Enum`). The checker splits the subject once into hidden locals, one per element and recursively one per element of a nested tuple (`explode_tuple`), then `match_parts` walks the pattern and the locals together: a value goes through the existing value test, an enum pattern through `case_patterns` (a tag test, with the payload bindings as statements that run in the body), a `let` binds a copy of the element, and the tests of all elements are joined with `&&`. The rules of the flat form carry over: a case with several tuple patterns cannot bind names, an arity mismatch is an error at any depth, and the names must be distinct.

**Consequences.** A nested pattern costs the hidden locals and one comparison per tested element. A switch whose last case contains an enum pattern or a value is not taken as matching every tuple, so such a switch needs a `default`; the exhaustiveness of enum cases inside tuples is not computed. Tuples still hold at most four elements. `case let (a, b)` is still unsupported.

### where on for and on case conditions is a rewrite in the parser (2026-11-27)

**Decision.** `for x in xs where cond { body }` (and the `for (k, v)` form) is parsed as a loop whose body is `if cond { body }`, and `case P = s where cond` in an `if`, `guard` or `while` is parsed as the two conditions `case P = s, cond`. No checker or code generation change: the filter sees the loop variable, a `continue` or labeled `break` in the body keeps its meaning, and the binding rules are the ones of a condition list.

**Consequences.** A filtered loop costs one comparison per element. Errors in the condition are reported against the line of the `for` or the condition list, not of a separate clause.


### A tuple holds two to five elements and one parenthesized name is a grouping (2026-11-27)

**Decision.** `MAX_TUPLE` (5) bounds the tuple types, literals and patterns. Tuple struct definitions are registered for two to five elements, and the weak and unowned cell definitions follow them in the definition table. A single element in parentheses is not a tuple, as in Swift: `(Int)` is `Int`, `(x)` is `x`, and `let (a) = e` or `var (a) = e` binds one name.

**Consequences.** A six-element tuple is rejected with a message naming the limit. A one-element tuple with a label (`(x: 1)`) is not supported.

### A local function that a function before it calls is declared up front (2026-11-27)

**Problem.** A local function saw only the local functions declared before it, so two local functions could not call each other.

**Options.** Declare every local function of a block at its start (Swift's rule, but a call before the definition runs reads an unset variable); declare only the functions that an earlier local function mentions; accept only a run of adjacent declarations.

**Decision.** `block` first scans its statements for a local function that an earlier local function of the same block names (`free::names_in`), and declares that name at the start of the block as a variable of its function type holding `Zero`, a null function value that retain and release already skip. The later declaration then assigns the closure to that variable instead of declaring a new one, so the earlier closure captures it through a cell like any variable, by reference, and sees the function once it is assigned. A statement that is not a local function and that names a function that is not assigned yet, directly or through an earlier local function that calls it, is rejected with "used before the local function that it needs is declared".

**Consequences.** Two local functions that call each other form a reference cycle through their cells and leak, as every strong cycle does. Only the functions that an earlier function names pay for the extra variable.

### A generic local function is a template that each call checks again with concrete types (2026-11-27)

**Problem.** A function type is a concrete signature, so a local function, which is a value, could not have type parameters.

**Options.** Make a generic local function a generic lifted function with its own type arguments, which needs the environment to be generic too; keep a template and check its body once per call with the type parameters bound to the inferred types.

**Decision.** A local function with type parameters is recorded as a template (`FnCx::templates`, registered at the start of its block and dropped when the block ends, and inherited by the closures it contains) and produces no statement. A call by name that is not a variable builds a signature from the template with its parameters appended to those of the enclosing function, checks the arguments with the existing closure-last rules, infers the arguments with `infer_from` (the enclosing parameters are bound to themselves), checks the bounds like a global call, then resolves the parameters and the result again with the type parameters bound to the concrete types and lifts the body as a plain local function, called where it is produced. Because the instance is an ordinary local function, a bound needs no witness: the operators of the concrete type apply. The names that the template mentions are captured by a closure that calls it, so a template can be called from a closure and from another template. A type parameter of the template hides a name of the same spelling in the enclosing scope while it is resolved.

**Consequences.** Every call lifts and compiles its own copy of the body and allocates an environment; two calls with the same types are not shared. A call from inside the body of the template reaches the instance being checked, so recursion works for the same type arguments only. A template cannot be passed as a value (`let g = twice`), and its associated-type projections (`T.Item`) are not resolved.

### Library members are Swift source that is added to a program when it uses their names (2026-11-27)

**Problem.** Array slicing members and `lazy` need generic code over closures and arrays, which the checker can only produce by building typed IR by hand for every member (as for `map` and `sorted`).

**Options.** Add more synthesized typed functions; write the members in the language itself and compile them like user code.

**Decision.** `src/array_prelude.swift` and `src/lazy_prelude.swift` hold ordinary generic functions and a generic struct. `prelude::extras` scans the tokens of the program: when a word such as `prefix`, `dropFirst`, `reversed` or `drop` appears the array source is appended to the program text, and when `lazy` appears the lazy source is. The result is lexed once more, so the added items are split, hashed, checked and cached like any other item, and their lines follow those of the program. The checker reaches them by name (`library_call`: the receiver goes first, the other arguments use the closure-last rules of a generic call, `finish_call` infers the type arguments): `a.prefix(2)` is `_arrayPrefix(a, 2)`, `s.prefix(while: p)` is `_stringPrefixWhile(s, p)`. A keyword may now label an argument and a parameter (`prefix(while:)`, `func f(while p: ...)`), because `while` is a keyword of the language. `dropFirst()` and `dropLast()` pass a count of one.

**Consequences.** A program that never uses these words pays nothing. A program that does adds the parsing of the library source (a few hundred lines), and every generic function in it is part of the signature digest. A user type named `LazySequence`, or a function named like a library function (they start with an underscore), collides with the library. `prefix`, `suffix`, `dropFirst`, `dropLast` and `drop(while:)` on an array copy the elements; there is no slice type, and a negative count is a fatal error. Arrays still have no `drop`/`prefix` that returns a view.

### A lazy sequence is a struct of a function that makes an iterator (2026-11-27)

**Problem.** `xs.lazy.map { ... }.filter { ... }.first` must evaluate the closures only for the elements that the result needs, and the sequence must be reusable. Arrays are always evaluated at once.

**Options.** Fuse a chain of members into one loop at check time (no type, but a chain cannot be held in a variable); give every stage a runtime representation.

**Decision.** `LazySequence<T>` (in the lazy source) has one member, `make: () -> () -> T?`: calling it returns a fresh iterator, a closure that returns the next element or `nil` and keeps its position in a captured variable. `_lazyArray` iterates over an array by index, and every member is a method that returns a new `LazySequence` whose `make` calls the `make` of the receiver: `map` and `filter` pull one element at a time, `prefix` stops pulling after its count, `prefix(while:)` after the first failure. Terminal members (`first(where:)`, `contains(where:)`, `reduce`, `forEach`, `allSatisfy`) pull until they know the answer. The checker adds four things: `.lazy` on an array, a string (its characters), a set (its keys), a dictionary (its `(key, value)` pairs) or a range, which converts the source to an array and calls `_lazyArray` (a member named `lazy` that a type declares still wins); the properties `first`, `count` and `isEmpty` (`_lazyFirst`, `_lazyCount`, `_lazyIsEmpty`); `for x in lazy`, rewritten to `let $iterator = $sequence.make()` and `while let x = $iterator() { ... }`, so `break` stops pulling; and `Array(lazy)` / `Set(lazy)` through `_lazyToArray`. A label `while:` or `where:` on a member of a lazy sequence selects the method named `prefixWhile`, `dropWhile`, `firstWhere` or `containsWhere`, since the language has no overloads.

**Consequences.** Each stage costs a heap environment and an indirect call per element. A source that is a range is turned into an array first (a range is not a value), so an infinite range (`(1...).lazy`) is still not supported, and the source of a lazy sequence is read when the iterator is made. Not provided on a lazy sequence: `min`, `max`, `sorted`, `reversed`, `contains(_:)`, `flatMap`, `suffix`, `zip`, `last`, a pair pattern in `for (a, b) in lazy`, and a lazy view of an existing lazy sequence by `.lazy`. `lazy` works through the closures of the language, so a stage that captures a variable by reference sees later changes of it.

### The ternary operator is a Select, and its mark is a question mark with a space on each side (2026-11-27)

**Problem.** `cond ? a : b` was missing. The token `?` already ends a type (`Int?`) and starts an optional chain (`x?.y`, `x?[i]`), so `c ? .red : .blue` and `c ? [1] : [2]` would be read as a chain.

**Options.** Look at what follows `?` and guess; give the lexer the whitespace rule of Swift (a ternary mark has whitespace on both sides, a chain or an optional type has none before it).

**Decision.** The lexer emits `lexer::TERNARY` (`"? "`) for a single `?` that has whitespace before and after it, and `?` for every other one, so the existing chain and type rules are untouched. `Parser::ternary` sits below `||` and above assignment (the lowest level, right associative): the condition, the mark on the same line, an expression, `:` and an expression, so `a ? b : c ? d : e` nests to the right and the `:` may start a new line. The checker lowers `ExprKind::Ternary` to `TExpr::Select`, which `??` and the dictionary lookup already use and which evaluates only the chosen branch. The condition must be `Bool`. The expected type goes to the first branch, and the type of the first branch goes to the second; a `nil` branch takes the optional of the other branch's type, so `c ? 1 : nil` is `Int?`. The result is the type that the other branch converts to (`c ? 1 : o` with `o: Int?` is `Int?`), and a mismatch is reported as `expected A, but found B`. Both branches are coerced to that type, so reference counting treats a borrowed and a freshly built value alike.

**Consequences.** `a ?b : c` and `a? b : c` are not ternaries. Because the token differs, `x ?.y` (a space before `?`) is no longer an optional chain, and the hash of an item changes when the spaces around a `?` change. A ternary whose two branches are both `nil` needs an optional type from its context. Branches that are closure literals need their parameter types written or taken from the context.

### A rebuild is bounded by the linker and the first read of a new file (2026-11-27)

**Problem.** On gen1000 a one-function edit cost 149 ms against 161 ms for a cold build, and a comment edit 62 ms of wall time for 20 ms of CPU time. Candidates named in the open decision: the pack file, the Cranelift backend, assembling and hashing the object, the linker.

**Measurement.** A temporary timer around every stage of `build_with` (release build, gen1000, ms): lex and split 7, digest, memo and pack load 4 to 5, parse of the changed function 1.5, check and resolve 0.15, backend construction 0.2, compile of the changed function 0.4, assemble 1.1, saving the pack and the memo 1.7, link 48 to 50; a cold build spends 69 on the link and 29 on parsing, checking and compiling 1000 functions in parallel. The linker is the fixed cost: `ld` takes 45 ms even for an object of 145 KB, and about 50 ms for an object with a single function, whatever the flags (`-no_uuid`, `-no_function_starts`, `-no_adhoc_codesign` change nothing); `-print_statistics` shows the time in reading `libSystem.tbd` and the dylibs that it re-exports. Separately, the first read of a file that was just written takes 21 to 29 ms on this machine (0.1 ms afterwards) because of the endpoint security software, so every edit build that follows a save pays it before apus starts working, and a cold build of an old file does not; together with the parallel cold build this is why an edit looked as costly as a cold build. A comment edit reuses the object, skips the link and costs 14 ms inside `build_with`.

**Options.** Link against a stub of `libSystem` that lists only the functions that the object imports; write the executable without `ld`; accept the cost.

**Decision.** `Linker::Ld` first links against `<cache>/libSystem.tbd`, a text stub generated from `codegen::LIBC_IMPORTS` (the same list that `assemble` declares as imports) with the install name `/usr/lib/libSystem.B.dylib`, so `dyld` still binds the symbols in the real library. If that fails it links with `-lSystem` as before and then with `cc`. `Linker::LdStub` is the stub alone, so a test can show that the fast path works and does not hide behind the fallback.

**Consequences.** `ld` takes 31 ms instead of 45 ms; a one-function edit on gen1000 went from 88 to 75 ms and a cold build from 94 to 84 ms in the same harness. A new libc function must be added to `LIBC_IMPORTS`, which is where it is declared anyway; if it is missing, the build falls back to the slower link and `the_minimal_system_library_stub_links_a_working_program` fails. What is left is `ld` itself (about 30 ms), which only a linker of our own would remove.

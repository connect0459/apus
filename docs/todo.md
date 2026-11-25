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
  - [x] several patterns in one `case` that bind the same names (`case .circle(let r), .square(let r):`)
  - [x] labels on payload values (`case rect(width: Int, height: Int)`), required where a case is built and optional in a pattern
  - [x] enum cases without the type name (`.red`, `.circle(5)`, `.some(4)`) where the expected type is known
  - [x] `String` operations: reference counting, `+` and `+=`, `count`, `isEmpty`, ordering and `Comparable`, `String(x)`, `hasPrefix`, `hasSuffix`, `contains` and interpolation

## Open decisions

- Rebuild timings were measured on 2026-11-20 with `just bench-vs-swiftc` (10 cores, load average 2.7 to 3.3, best of 5 runs for apus and 3 for `swiftc -Onone`; wall ms):

  | | gen1000 apus | gen1000 swiftc | gen5000 apus | gen5000 swiftc |
  | :--- | ---: | ---: | ---: | ---: |
  | cold build | 161 | 3455 | 275 | 11379 |
  | edit 1 function | 149 | 3499 | 274 | 11123 |
  | edit a comment | 62 | | 199 | |
  | no change | 7 | | 11 | |

  Open question raised by the numbers: editing one function (149 ms) costs almost as much as a cold build (161 ms), and a comment edit takes 62 ms of wall time for 20 ms of CPU time although nothing is compiled or linked. The cost does not scale with the number of changed functions, so something proportional to program size dominates: candidates are loading and saving the pack file, building the Cranelift backend, assembling and hashing the whole object, and linking. Profile these stages before optimizing anything.
- Statements end at a newline, `;` or `}`; a binary operator must stay on the same line as its left operand.
- Wrapping operators (`&+`, `&*`) and overflow trapping are not decided.
- Entry point is `func main()` instead of top-level statements.
- Linking cost with the system linker on macOS is unmeasured.
- Coverage gate stays at 80% line / 90% function; branch coverage is not gated because the stable toolchain cannot measure it. Baseline on 2026-11-20 (`cargo llvm-cov`): 95.9% lines and 93.6% functions. The weakest files are `link.rs` (50% of functions: the fallback and error paths of the system linker) and `codegen.rs` (75% of functions: the error conversions). Compare against these numbers when a change lowers coverage.
- Known gaps in the M2 language: `String` has no operation on its characters (indexing, slicing, `split`, `uppercased`, `+` with a `Character`), its `count` is the number of UTF-8 bytes, a value of a struct, a class or an optional cannot be converted to `String`, an enum with payloads cannot be printed, `switch` works only on enums, `Int`, `Bool` and `String` and a case value must be a literal, generic arguments cannot be structs or enums with payloads where they end up as a struct field or a payload, a value of a type parameter without a constraint can only be copied, passed, returned and stored, an expected type reaches only the top expression of a `let` initializer or a `return`, payloads at the same position of different cases must have one type when it is a class or a struct, an initializer cannot be overridden or inherited so a subclass of a class with an `init` must declare its own, `super.init` builds a separate superclass instance and copies its members so a method called from a superclass initializer dispatches on the superclass, `super` is accepted only as `super.method(...)` and `super.init(...)`, implicit `self` applies to a name only inside a method or an initializer and not to a member of another value, an initializer cannot have type parameters, a mutating method of an enum without payloads must be called as a statement of its own, once per statement, on a variable, an initializer counts only top-level `self.member = value` statements whose value does not mention `self`, a member path that crosses an `unowned` or `weak` member cannot be assigned through, `unowned` applies only to a class member and not to a local, a parameter or a struct member, `weak` applies only to a `var` class member of an optional class type, a cycle of strong references leaks, a generic class cannot declare a `deinit`, a variable is released at the end of its scope and not after its last use, an argument is borrowed for the call so a callee that drops the last other reference to it is undefined behavior, and a `case` may repeat a pattern without a warning.

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

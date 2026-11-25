# Todo

## Milestones

- [x] M0: benchmark generator (`bench/gen.py`) and baseline against `swiftc -Onone`
- [ ] M1: front end
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
- [ ] M4: enum, generics (monomorphization), simple class
  - [x] Enum without payloads: `enum`, `Enum.case`, `==`/`!=`, enum struct fields, exhaustive `switch`
  - [x] Enum with payloads: `Shape.circle(5)`, `case .circle(let r)`, `_` to skip a value
  - [x] Generics by monomorphization: generic functions, structs and enums, inferred type arguments
  - [x] Simple class: reference semantics, memberwise init, mutable members, generic classes; instances are never freed
  - [ ] Reference counting for classes, with `deinit`, `weak` and `unowned` (see the class decision)
  - [x] Methods and `mutating` methods on structs, classes and enums; `init` on structs and classes with definite initialization
  - [x] Methods and initializers in generic types
  - [x] Implicit `self` for member reads, member assignments and method calls
  - [x] Generic methods: a method may declare type parameters of its own
  - [x] `mutating` on enums without payloads, `init` on enums
  - [ ] Inheritance and `===`

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
- Known gaps in the M2 language: equality on `String` and structs, `Int.min % -1` does not trap, structs cannot contain structs, `String` has no operations beyond literals and `print`, an enum value cannot be printed, `switch` works only on enums, generic arguments cannot be structs or enums with payloads where they end up as a struct field or a payload, there are no protocol constraints so a value of a type parameter can only be copied, passed, returned and stored, explicit type arguments are not accepted on function calls, an expected type reaches only the top expression of a `let` initializer or a `return`, payloads are unlabeled and limited to `Int`, `Bool`, `String` and enums without payloads, a class has no inheritance and no `===`, implicit `self` applies to a name only inside a method or an initializer and not to a member of another value, an initializer cannot have type parameters, explicit type arguments are not accepted on a method call, a type has at most one `init`, a mutating method of an enum without payloads must be called as a statement of its own, once per statement, on a variable, an initializer counts only top-level `self.member = value` statements whose value does not mention `self`, a class member cannot be a struct, a member of a class is assignable only through a variable (`c.a.b = 1` is rejected), a class cannot refer to itself because there is no optional, a case with bindings has a single pattern, `.circle(5)` without the type name is not accepted, and a `case` may repeat a pattern without a warning.

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

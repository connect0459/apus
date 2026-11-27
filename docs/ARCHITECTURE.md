# Architecture

## Bird's Eye View

apus compiles a Swift subset (a dialect, not Swift) to a native macOS arm64 executable. Its goal is a fast edit-and-rebuild loop: after editing one function, only that function is recompiled.

The unit of incremental work is the **item**: a top-level `func`, `struct`, `class` or `enum`, or a method or initializer of a type, which `items` splits out of its type body. Everything is organized so that an item can be hashed, parsed, checked and compiled without looking at the bodies of other items.

```text
source → lexer → items → parser → check → mono → codegen → link → driver
                                           ↑
                                  cache (function entries)
```

All stages exist. `codegen` is split into `compile_function`, which turns one typed function into a cacheable entry, and `assemble`, which builds the object file from entries. See `todo.md` for what is planned.

## Code Map

**`lexer`** turns source text into tokens. Comments and whitespace produce no tokens, but every token remembers its line. The parser relies on lines to decide where statements end.

**`prelude`** looks at the tokens of a program and, when it uses a word of the library (`lazy`, `prefix`, `dropFirst`, `reversed` ...), returns Swift source for the library members (`array_prelude.swift`, `lazy_prelude.swift`). The driver appends it to the program text and lexes the result again, so the library is split, hashed, checked and cached like user code.

**`items`** splits the token stream into top-level items, takes the methods and initializers out of each type body as items of their own, and computes a content hash for each. It is the only stage that sees the whole file; later stages receive one item at a time.

**`ast` and `parser`** define the syntax tree and parse a single item into it. The parser never looks at tokens outside the item it was given. Every statement and expression remembers its line.

**`types`, `sigs` and `check`** hold the type system and the checker. `Signatures` (in `sigs`) is built once from the declarations of all items; it also interns the instances of generic structs and enums, so it grows while functions are checked. `check_func` checks one function using only that function's body and the signatures, and produces typed IR. A generic function is checked once with opaque type parameters (`Ty::Param`), and calls to generic functions record their `type_args`. The only whole-program information a function body can see is a signature, never another body.

**`mono`** turns calls with concrete type arguments into instances: it substitutes the types of the generic function's typed IR, names the instance `name<Type,...>`, and does the same for the calls inside it, one instance per name. An instance is an ordinary typed function from then on.

**`init_check`** decides, from the syntax of an initializer and the names of the type's members, methods and the initializer's parameters, whether it sets every member before using `self`, with or without writing `self.`.

**`tir`** is the typed IR that `check` produces and `codegen` consumes. Every expression carries its type, so `codegen` never infers anything.

**`codegen`** turns typed IR into a Mach-O object through Cranelift, with no optimization. `compile_function` is a pure function of one typed function and the struct layouts. It returns a `FuncEntry` (machine code, relocations that name their targets by symbol, and the string literals it uses) without touching any shared state, so entries can be cached and compiled in parallel. `assemble` defines the entries in one object file and emits the C `main` that calls the program's `main`.

**`key`** computes the cache key of a function: a stable hash of its typed IR, with struct types encoded by their field types rather than by declaration order, plus a stamp identifying the compiler.

**`entry`, `cache` and `stamp`** hold the persistent state under `.apus-cache/` next to the output. `entry` serializes one function. `cache` keeps entries in one pack file and saves only the entries the latest build used, so the cache never outgrows the program. `stamp` records the last build (source hash, object hash and the output file's size and modification time) so an unchanged build can stop immediately.

**`memo`** stores, next to the cache, which function keys each item hash produced in the last successful build (its own entry and the generic instances it needs) and the signature digest it was built under. The driver trusts it only while the compiler stamp and the digest match, and then skips parsing and checking of functions it still knows.

**`link`** runs the system linker on the object file: `ld` called directly with the SDK location cached, falling back to `cc`.

**`driver`** wires the stages together and reports errors with their line. `src/main.rs` is the command line on top of it.

## Invariants

- An item's hash depends only on its normalized tokens. Whitespace, comments and line shifts never change it.
- Hashes are persisted, so the hash function must be deterministic across runs and Rust versions.
- The parser is stateless across items. Parsing item A never depends on item B.
- Checking a function depends only on its own body and on signatures.
- Syntax nodes compare by shape; their lines are metadata and do not affect equality.
- Layers depend only on earlier layers. Nothing points backwards.
- Struct fields are private. Data is read through methods; behavior lives on the type that owns the data. Fields of enum variants are public because pattern matching is the interface of a sum type.
- A generic function is an exception to the callee-body rule below: its whole text is part of the signature digest, so editing it rechecks every function. Its instances are still recompiled only when their typed IR changes.
- A cache key depends only on a function's typed IR and the layouts of the structs it uses. Editing a callee's body therefore never invalidates its callers; changing a callee's signature does, because the caller's typed IR changes.
- Compilation of a function is deterministic and independent of other functions, so parallel and sequential builds produce identical entries.
- Cache files are untrusted input: a corrupt or truncated file is ignored, never trusted partially.
- An enum without payloads is a 64-bit integer holding the case index. An enum whose cases carry values is a struct of one tag slot and as many 8-byte slots as its widest case, so it is copied and passed like a struct. `switch` has no typed IR node of its own: `check` lowers it to a temporary local and a chain of `if` statements, with payload bindings as `let` statements at the start of each case.
- Integer arithmetic traps on overflow and on division by zero, as Swift does.
- A struct is a value. It lives in memory as one 8-byte slot per field, and an expression of struct type evaluates to the address of its storage. A struct is copied at `let` and assignment. A parameter is passed as a pointer and is immutable, so the callee never changes the caller's value. A result is written to memory supplied by the caller.

- A class is a heap block of one 8-byte slot per member behind a 64-bit pointer (`Ty::Class`). It is copied, passed and returned as a scalar, `TExpr::New` allocates it with `malloc`, and nothing frees it. Members are read and assigned through the pointer, so a copy of the reference shares the instance.

- A method is the function `Type.method` with `self` as its first parameter, an initializer is `Type.init` returning the type, and a call through a value is lowered by `check` to a plain call. A `mutating` method receives the address of the caller's variable. An initializer starts from a zero-filled `self` and `init_check` guarantees that no zero is observed.
- A member's hash includes its owner, and the signature digest includes its header but not its body, except for the members of a generic type: those are generic functions over the type's parameters, so their whole text is part of the digest and `mono` instantiates them per type argument.

## Cross-Cutting Concerns

**Dialect.** Statements end at a newline, `;` or `}`. A binary operator must stay on the same line as its left operand. The entry point is `func main()`. Type inference flows from an initializer only; there are no overloads, and integer literals are always `Int`. Argument labels must match the declaration exactly.

**Errors.** Every stage returns a `Result` whose error carries the line of the offending token or node.

**Performance.** The reference point is `swiftc -Onone`. On the generated 1000-function program in `bench/`, apus builds in about 115 ms against 2.3 s. Of that, about 6 ms is parsing, 2 ms is checking, 47 ms is Cranelift code generation and 56 ms is linking through `cc`. Each stage must justify its cost against these numbers.

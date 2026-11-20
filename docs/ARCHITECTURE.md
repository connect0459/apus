# Architecture

## Bird's Eye View

apus compiles a Swift subset (a dialect, not Swift) to a native macOS arm64 executable. Its goal is a fast edit-and-rebuild loop: after editing one function, only that function is recompiled.

The unit of incremental work is the top-level **item** (a `func`, a `struct` or an `enum`). Everything is organized so that an item can be hashed, parsed, checked and compiled without looking at the bodies of other items.

```text
source → lexer → items → parser → check → codegen → link → driver
                                           ↑
                                  cache (function entries)
```

All stages exist. `codegen` is split into `compile_function`, which turns one typed function into a cacheable entry, and `assemble`, which builds the object file from entries. See `todo.md` for what is planned.

## Code Map

**`lexer`** turns source text into tokens. Comments and whitespace produce no tokens, but every token remembers its line. The parser relies on lines to decide where statements end.

**`items`** splits the token stream into top-level items and computes a content hash for each. It is the only stage that sees the whole file; later stages receive one item at a time.

**`ast` and `parser`** define the syntax tree and parse a single item into it. The parser never looks at tokens outside the item it was given. Every statement and expression remembers its line.

**`types` and `check`** hold the type system and the checker. `Signatures` is built once from the declarations of all items. `check_func` then checks one function using only that function's body and the signatures, and produces typed IR. The only whole-program information a function body can see is a signature, never another body.

**`tir`** is the typed IR that `check` produces and `codegen` consumes. Every expression carries its type, so `codegen` never infers anything.

**`codegen`** turns typed IR into a Mach-O object through Cranelift, with no optimization. `compile_function` is a pure function of one typed function and the struct layouts. It returns a `FuncEntry` (machine code, relocations that name their targets by symbol, and the string literals it uses) without touching any shared state, so entries can be cached and compiled in parallel. `assemble` defines the entries in one object file and emits the C `main` that calls the program's `main`.

**`key`** computes the cache key of a function: a stable hash of its typed IR, with struct types encoded by their field types rather than by declaration order, plus a stamp identifying the compiler.

**`entry`, `cache` and `stamp`** hold the persistent state under `.apus-cache/` next to the output. `entry` serializes one function. `cache` keeps entries in one pack file and saves only the entries the latest build used, so the cache never outgrows the program. `stamp` records the last build (source hash, object hash and the output file's size and modification time) so an unchanged build can stop immediately.

**`memo`** stores, next to the cache, which function key each item hash produced in the last successful build and the signature digest it was built under. The driver trusts it only while the compiler stamp and the digest match, and then skips parsing and checking of functions it still knows.

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
- A cache key depends only on a function's typed IR and the layouts of the structs it uses. Editing a callee's body therefore never invalidates its callers; changing a callee's signature does, because the caller's typed IR changes.
- Compilation of a function is deterministic and independent of other functions, so parallel and sequential builds produce identical entries.
- Cache files are untrusted input: a corrupt or truncated file is ignored, never trusted partially.
- An enum without payloads is a 64-bit integer holding the case index. An enum whose cases carry values is a struct of one tag slot and as many 8-byte slots as its widest case, so it is copied and passed like a struct. `switch` has no typed IR node of its own: `check` lowers it to a temporary local and a chain of `if` statements, with payload bindings as `let` statements at the start of each case.
- Integer arithmetic traps on overflow and on division by zero, as Swift does.
- A struct is a value. It lives in memory as one 8-byte slot per field, and an expression of struct type evaluates to the address of its storage. A struct is copied at `let` and assignment. A parameter is passed as a pointer and is immutable, so the callee never changes the caller's value. A result is written to memory supplied by the caller.

## Cross-Cutting Concerns

**Dialect.** Statements end at a newline, `;` or `}`. A binary operator must stay on the same line as its left operand. The entry point is `func main()`. Type inference flows from an initializer only; there are no overloads, and integer literals are always `Int`. Argument labels must match the declaration exactly.

**Errors.** Every stage returns a `Result` whose error carries the line of the offending token or node.

**Performance.** The reference point is `swiftc -Onone`. On the generated 1000-function program in `bench/`, apus builds in about 115 ms against 2.3 s. Of that, about 6 ms is parsing, 2 ms is checking, 47 ms is Cranelift code generation and 56 ms is linking through `cc`. Each stage must justify its cost against these numbers.

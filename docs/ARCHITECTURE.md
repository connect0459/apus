# Architecture

## Bird's Eye View

apus compiles a Swift subset (a dialect, not Swift) to a native executable. Its goal is a fast edit-and-rebuild loop: after editing one function, only that function is recompiled.

The unit of incremental work is the top-level **item** (a `func` or a `struct`). Everything is organized so that an item can be hashed, parsed, checked and compiled without looking at the bodies of other items.

```text
source → lexer → items → parser → check → codegen → cache → driver
```

Only the first three stages exist so far. See `todo.md` for what is planned.

## Code Map

**`lexer`** turns source text into tokens. Comments and whitespace produce no tokens, but every token remembers its line. The parser relies on lines to decide where statements end.

**`items`** splits the token stream into top-level items and computes a content hash for each. It is the only stage that sees the whole file; later stages receive one item at a time.

**`ast` and `parser`** define the syntax tree and parse a single item into it. The parser never looks at tokens outside the item it was given.

## Invariants

- An item's hash depends only on its normalized tokens. Whitespace, comments and line shifts never change it.
- Hashes are persisted, so the hash function must be deterministic across runs and Rust versions.
- The parser is stateless across items. Parsing item A never depends on item B.
- Layers depend only on earlier layers. Nothing points backwards.
- Struct fields are private. Data is read through methods; behavior lives on the type that owns the data.

## Cross-Cutting Concerns

**Dialect.** Statements end at a newline, `;` or `}`. A binary operator must stay on the same line as its left operand. The entry point is `func main()`.

**Errors.** Every stage returns a `Result` whose error carries the line of the offending token.

**Performance.** The reference point is `swiftc -Onone`, which needs 3.7 to 6.9 seconds for the 1000-function benchmark in `bench/`. Each stage must justify its cost against that number.

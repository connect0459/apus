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
- Coverage gate stays at 80% line / 90% function; branch coverage is not gated.
- Known gaps in the M2 language: equality on `String` and structs, `Int.min % -1` does not trap, structs cannot contain structs, `String` has no operations beyond literals and `print`.

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

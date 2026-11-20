# Todo

## Milestones

- [x] M0: benchmark generator (`bench/gen.py`) and baseline against `swiftc -Onone`
- [ ] M1: front end
  - [x] Lexer
  - [x] Item splitting and hashing
  - [ ] Parser (func, struct, Int/Bool, if/for/while)
- [ ] M2: type checking, Cranelift aarch64 object output, link with the system linker, run
- [ ] M3: per-function disk cache; editing one function recompiles only that function
- [ ] M4: enum, generics (monomorphization), simple class

## Open decisions

- Entry point is `func main()` instead of top-level statements.
- Linking cost with the system linker on macOS is unmeasured.
- Coverage gate stays at 80% line / 90% function; branch coverage is not gated.

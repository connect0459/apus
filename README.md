# apus

[![CI](https://github.com/connect0459/apus/actions/workflows/ci.yml/badge.svg)](https://github.com/connect0459/apus/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](https://github.com/connect0459/apus/blob/main/LICENSE)

An experiment: can a compiler for a Swift subset, built around per-item hashing and a per-function cache, make the edit-and-rebuild loop an order of magnitude faster than `swiftc`?

> **Status**: experimental. Targets macOS arm64 only. The accepted language is a dialect of Swift, not Swift.

## Approach

- Split the source into top-level items and hash their normalized tokens, so unchanged items are never parsed.
- Cache machine code per function, keyed by the function's own hash and the signatures it references, so editing a callee's body does not recompile its callers.
- Emit unoptimized code through Cranelift.

## Measurements

| Program | `swiftc -Onone` (cold) | apus front end (lex + split) |
| :--- | ---: | ---: |
| 1000 functions (~110 KB) | 3.7–6.9 s | 4.8 ms |
| 5000 functions (~570 KB) | not measured | 28 ms |

The front end alone is not a compiler; the comparison only bounds what is achievable. See `docs/todo.md` for milestones.

## Documentation

- [ARCHITECTURE.md](https://github.com/connect0459/apus/blob/main/docs/ARCHITECTURE.md)

## Contributing

See [CONTRIBUTING.md](https://github.com/connect0459/apus/blob/main/CONTRIBUTING.md).

## License

[MIT](https://github.com/connect0459/apus/blob/main/LICENSE)

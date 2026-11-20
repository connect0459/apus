# apus

[![CI](https://github.com/connect0459/apus/actions/workflows/ci.yml/badge.svg)](https://github.com/connect0459/apus/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](https://github.com/connect0459/apus/blob/main/LICENSE)

An experiment: can a compiler for a Swift subset, built around per-item hashing and a per-function cache, make the edit-and-rebuild loop an order of magnitude faster than `swiftc`?

> **Status**: experimental. Targets macOS arm64 only. The accepted language is a dialect of Swift, not Swift.

## Usage

```sh
cargo build --release
target/release/apus build hello.swift -o hello
./hello
```

A program is a file of top-level `func` and `struct` declarations with a `func main()`:

```swift
struct Point { var x: Int; var y: Int }

func manhattan(_ p: Point) -> Int { return p.x + p.y }

func main() {
  let p = Point(x: 3, y: 4)
  print(manhattan(p))
}
```

## Language

- Types: `Int`, `Bool`, `String` (literals and `print` only), and structs with `Int`, `Bool` and `String` fields.
- Statements: `let`, `var`, assignment (including `+=` and friends), `if`/`else`, `while`, `for` over `a..<b` and `a...b`, `return`.
- Integer arithmetic traps on overflow and division by zero, as in Swift.
- It is a dialect: there are no overloads, integer literals are always `Int`, and a binary operator must stay on the same line as its left operand.

## Approach

- Split the source into top-level items and hash their normalized tokens, so unchanged items are never parsed.
- Check and compile one function at a time, so the cost of an edit follows the size of the edit.
- Emit unoptimized code through Cranelift.

## Measurements

Cold builds of generated programs (`python3 bench/vs_swiftc.py`, Apple Silicon, best of several runs). The two programs print the same output.

| Program | `swiftc -Onone` | apus |
| :--- | ---: | ---: |
| 1000 functions (~110 KB) | 2.3 s | 115 ms |
| 5000 functions (~570 KB) | 10.9 s | 350 ms |

Of the 115 ms, linking through `cc` takes 56 ms and Cranelift 47 ms. Rebuilds are not faster yet: there is no cache. See `docs/todo.md` for milestones.

## Documentation

- [ARCHITECTURE.md](https://github.com/connect0459/apus/blob/main/docs/ARCHITECTURE.md)

## Contributing

See [CONTRIBUTING.md](https://github.com/connect0459/apus/blob/main/CONTRIBUTING.md).

## License

[MIT](https://github.com/connect0459/apus/blob/main/LICENSE)

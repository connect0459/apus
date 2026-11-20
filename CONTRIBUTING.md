# Contributing

## Prerequisites

- [rustup](https://www.rust-lang.org/tools/install) — installs the exact Rust toolchain pinned in `rust-toolchain.toml` automatically
- [just](https://just.systems/) — task runner
- [pre-commit](https://pre-commit.com/) — hook runner
- macOS on Apple Silicon with the Xcode command line tools (the backend targets the host and links with the system linker)

## Setup

```sh
pip install pre-commit   # or: brew install pre-commit
git clone https://github.com/connect0459/apus.git
cd apus
just setup
```

## Development workflow

| Command | Purpose |
| :--- | :--- |
| `cargo test --all-targets` | Run all tests |
| `cargo fmt` | Format all Rust source files |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint Rust code |
| `just bench-front` | Measure the front end on a generated program |
| `just verify` | Run the full CI-equivalent check locally |

The pre-commit hooks enforce formatting and lint checks on every commit. To run them manually across all files:

```sh
pre-commit run --all-files
```

## Testing guidelines

This project follows **Red → Green → Refactor** (Detroit-school TDD):

- Write a failing test first, then implement.
- Use real objects; mocks are only permitted at external boundaries (file system, linker, network).
- Test names describe **what rule** is verified, not how.
- Coverage targets: 80% line, 75% branch, 90% function.

Unit tests live in-file under `#[cfg(test)] mod tests`; integration tests live under `tests/`.

## Commit format

```text
<type>(<scope>): <subject>
```

**Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `tidy`, `test`, `chore`, `ci`, `perf`

**Subject**: imperative mood, 72 characters max, no trailing period.

Examples:

```text
feat(lexer): add nested block comments
perf(items): hash tokens without allocating
tidy(items): remove unused import
```

## Pull request process

1. Fork the repository and create a branch: `feat/xxx`, `fix/xxx`, `docs/xxx`.
2. Follow the Red → Green → Refactor cycle.
3. Run `just verify` and commit any resulting diffs.
4. If the change is an architectural decision, record it under Decisions in `docs/todo.md`.
5. Open a pull request — CI runs fmt, clippy, tests, and the coverage gate.

## Code style

- No code comments unless the **why** is genuinely non-obvious.
- Respect layer boundaries (see `AGENTS.md`).
- Prefer immutability; avoid mutable state unless necessary.
- Use `Result` for error handling; avoid `unwrap()`/`expect()` outside tests and examples.
- All user-facing strings (test names, error messages) must be in **English**.

# Project Agents.md Guide

This is a Rust project: **apus**, an experimental compiler for a Swift subset (a dialect) built for fast incremental builds. Its design rests on four ideas: its own front end, per-item content hashing, a per-function cache, and a cheap non-optimizing backend (Cranelift).

> **Status**: experimental. It targets macOS arm64 only and is not a replacement for `swiftc`.

## Language Convention

This project is intended for open-source distribution. All of the following must be written in **English**:

- Commit messages
- Documentation (including `AGENTS.md`, `README.md`, etc.)
- Test names
- Error messages
- Variable/function names

## Project Structure

The compiler is a pipeline of layers; each layer depends only on the layers before it:

```text
lexer → items (split and hash) → parser → check → codegen → cache → driver (CLI)
```

Respect these layer boundaries: never introduce a dependency that points backwards (e.g., the lexer depending on the parser). See `docs/ARCHITECTURE.md` for the bird's-eye view. `docs/todo.md` holds the milestones, the open questions and the record of design decisions; record new decisions there.

## Coding convention

- Format with `cargo fmt`; pass `cargo clippy --all-targets --all-features -- -D warnings`.
- Use `Result` for error handling; avoid `unwrap()`/`expect()` in production code.
- Keep struct fields private. Expose data through methods named after the property (`name()`, not `get_name()`) and put behavior on the type that owns the data. Make a field `pub` only when there is no alternative.
- Enum variant fields are public by language rule; pattern matching is the interface of a sum type such as `Stmt` and `Expr`.
- Avoid `unsafe` unless the reason is recorded under Decisions in `docs/todo.md`.
- Cache keys must be deterministic across runs and Rust versions; do not use `std::hash::DefaultHasher` for anything persisted.

## Tooling

- `cargo build` / `cargo test --all-targets` — build and test.
- `just verify` — run the full CI-equivalent check (fmt, clippy, test, coverage) before opening a PR.
- `just bench-front` — measure the front end on a generated 1000-function program.
- **pre-commit hooks** — catch formatting and lint issues on every commit (`pre-commit run --all-files` to run manually).

Baseline to keep in mind: `swiftc -Onone` takes 3.7–6.9 s to build the 1000-function program from `bench/gen.py`. Every stage added to the pipeline must justify its cost against that number.

## Development Philosophy

### Red/Green TDD (Detroit school)

- Red → Green → Refactor cycle strictly followed.
- Use real objects; mocks are only permitted at external boundaries (file system, linker, network).
- Write tests BEFORE implementation; run tests AFTER implementation.
- Coverage targets: 80% line, 75% branch, 90% function.

### Domain Object Design

- Rich domain objects: pair data and logic in the same type.
- Prefer immutability; avoid mutable state unless necessary.
- Distinguish entities (identity-based) from value objects (value-based).

### Evergreen Tests

- Test names describe WHAT rule is being verified, not HOW.
- Test names must not reference implementation details.
- Test code serves as living documentation of the system's behavior.

### Code Comments

- Do NOT write code comments unless explicitly permitted by the user.
- Let the code speak for itself; let tests document the behavior.
- Code = How, Tests = What, Commit messages = Why.

## Git Conventions

### Format

```text
<type>(<scope>): <subject>

<body>

<footer>
```

### Types

| Type | Description |
| :--- | :--- |
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Code style (formatting, whitespace) |
| `refactor` | Code change that is neither a fix nor a feature |
| `tidy` | Small, safe cleanup (< 2 min; no behavior change) |
| `test` | Adding or updating tests |
| `chore` | Build process, tooling, or config changes |
| `ci` | CI/CD pipeline changes (GitHub Actions, workflows) |
| `perf` | Performance improvement |

### Scopes

Scope is optional; use the layer name when the change targets a specific part of the codebase (e.g., `lexer`, `items`, `parser`, `codegen`, `cache`). Omit for project-wide changes.

### Type vs. Scope Precedence

The type vocabulary above mixes two axes: an **impact axis** (`feat`, `fix`, `perf`, `refactor`) and a **domain axis** (`docs`, `style`, `test`, `chore`, `ci`, `tidy`). When a change is fully contained within a domain, use that domain as `type` (e.g. `docs: fix typo`); do not use it as `scope` on an impact-axis type (avoid `fix(docs): ...`). Dependency updates use `chore` or `build`, never `fix(deps)`.

### Subject Line

- Use the imperative mood: "add", "fix", "remove" — not "added" or "adds".
- 72 characters max.
- No trailing period.

### Body (optional)

- Explain **why**, not what — the diff already shows what changed.

### Footer (optional)

- `BREAKING CHANGE: <description>` for breaking changes.
- `Closes <https://github.com/owner/repo/issues/123>` to link issues; never use the `#123` form.

### Branch naming

`feat/xxx`, `fix/xxx`, `docs/xxx`.

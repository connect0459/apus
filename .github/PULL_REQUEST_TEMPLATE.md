<!-- # PULL_REQUEST_TEMPLATE -->

<!-- Remove unnecessary sections to keep the review focused -->

## Related Links

- Issues
  - <!-- <https://github.com/connect0459/apus/issues/xxx> -->
- PRs
  - <!-- <https://github.com/connect0459/apus/pull/xxx> -->

## [Required] Overview

- Describe the problem being solved, its background, and what changes when this PR is merged.
- Links to `docs/todo.md` entries or other references are welcome.

## Scope of Change

- [ ] Front end (lexer, items, parser)
- [ ] Type checking
- [ ] Code generation / linking
- [ ] Incremental cache
- [ ] Tooling / CI
- [ ] Documentation

## Breaking Changes

- [ ] No breaking changes
- [ ] Breaking changes (describe below)

## Deferred Items and TODOs

- Items intentionally deferred and the reasons why.

## Test Items

- Describe the tests added, following Red/Green TDD (which test was written first, and what it confirmed failed before the implementation existed).
- Note coverage if it changed meaningfully (targets: 80% line, 75% branch, 90% function).
- Note benchmark results if the change affects build speed (`just bench-front`).
- Confirm `just verify` passes with no regressions.

## [Required] Quality Checklist

**Please check all items before merging.**

- [ ] **CI Workflow Execution**: All checks passed on the [CI workflow](../actions/workflows/ci.yml) for this PR.
- [ ] **Code Comments**: None, except non-obvious WHY/WHY-NOT explanations, per this project's comment policy.
- [ ] **Reference Docs**: `docs/todo.md` updated to check off completed items and record any design decisions made along the way.

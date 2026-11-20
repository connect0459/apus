# Security Policy

## Supported Versions

Only the latest release on the `main` branch is actively maintained. Older versions do not receive security fixes.

| Version  | Supported |
| :------- | :-------- |
| latest   | ✓         |
| < latest | ✗         |

## Reporting a Vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Use GitHub's [private vulnerability reporting][private-report] feature to disclose issues confidentially. You will receive an acknowledgment within **5 business days** and a resolution timeline once the report has been triaged.

[private-report]: https://github.com/connect0459/apus/security/advisories/new

## Scope

The following vulnerability classes are in scope for this project:

- **Compiler crashes** — any input that causes a panic, SIGSEGV, or unhandled runtime error in the lexer, parser, or type checker.
- **Resource exhaustion** — inputs that cause unbounded memory growth, CPU spin, or stack overflow in the compiler.
- **Cache poisoning** — any way for crafted source or cache contents to make the compiler emit machine code that does not correspond to the given source.
- **Unsafe file handling** — path traversal or unintended writes outside the output and cache directories.

The following are **out of scope**:

- Issues in third-party dependencies (report those upstream).
- Incorrect compilation results that are not security-relevant (report those as regular bugs via GitHub Issues).
- Theoretical issues without a reproducible proof-of-concept.

## Disclosure Policy

Once a fix is ready and released, a GitHub Security Advisory will be published with full details. The typical timeline from report to public disclosure is **30 days**, though this may be extended by mutual agreement when a fix requires significant changes.

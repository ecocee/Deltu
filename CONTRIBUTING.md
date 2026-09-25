# Contributing to Deltu

Thank you for considering a contribution. This document explains how
contributions work and how intellectual property is handled. It is
project guidance, not legal advice.

## License

Deltu is licensed under the **Apache License 2.0** (see `LICENSE`).
By contributing, you agree that your contributions are provided under
Apache-2.0, per the License's §5 ("Submission of Contributions") and
`NOTICE`.

**You keep the copyright in what you write.** Submitting a contribution
does not transfer ownership of it to ECOCEE or anyone else: Apache-2.0
is a license grant, not an assignment. ECOCEE holds copyright in the
portions it authored; each contributor retains copyright in their own
portions, and everything is collectively usable by everyone under
Apache-2.0.

## Contributor License Agreement (CLA)

There is **no CLA in effect today**. Submissions are accepted under
Apache-2.0's default §5 terms.

If the project later adopts a CLA or a Developer Certificate of Origin
(DCO) workflow — for example, to let ECOCEE relicense or to structure
commercial arrangements — that will be announced here and enforced for
contributions made *after* the change. Contributions already merged
under §5 are not retroactively affected. Any future CLA will be
reviewed for compatibility with this document before adoption.

## What we accept

- Bug fixes with a regression test where practical.
- Features aligned with the build plan's scope (see
  `context/specs/`); open an issue first for anything architectural.
- Documentation improvements.

Every engine change must pass the project's verification checklist:

```bash
cargo fmt --check
cargo clippy --all-targets   # zero warnings
cargo test --lib
```

Design rules the code review will hold you to: bounded memory by
construction, no processing logic in I/O handlers, no unbounded
background growth, no new dependencies without a stated justification,
AI functionality stays behind the provider boundary (spec 11).

## Process

1. Open or comment on an issue describing the change.
2. Fork/branch from the current development branch.
3. Make the change with tests; follow the existing code style.
4. Run the checklist above.
5. Open a pull request describing the *why*, not just the *what*.

By submitting a pull request or patch, you affirm that the
contribution is your own work or that you have the right to submit it
under Apache-2.0.

## Reporting issues

Security-sensitive behavior (injection at a boundary, crashes,
deadlocks): please mark issues clearly and avoid publishing exploit
details until a fix is available.

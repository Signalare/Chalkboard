# Contributing to Chalkboard

Thanks for your interest in Chalkboard! This guide explains how to propose
changes and what we expect from a pull request.

By participating you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).
Security issues follow a separate, private process — see [SECURITY.md](SECURITY.md).

> **Project status:** Chalkboard is in the design phase. The
> [specification](SPEC.md) is a draft and the toolchain described below is
> being set up alongside the first code. Where a tool or check does not exist
> yet, follow the intent of the rule.

## Table of contents

- [Ways to contribute](#ways-to-contribute)
- [Before you start: issue first](#before-you-start-issue-first)
- [Developer Certificate of Origin](#developer-certificate-of-origin)
- [Branches and pull requests](#branches-and-pull-requests)
- [Commit messages](#commit-messages)
- [Changelog](#changelog)
- [Code style and checks](#code-style-and-checks)
- [Testing](#testing)
- [Changing the specification](#changing-the-specification)
- [AI-assisted contributions](#ai-assisted-contributions)
- [Licensing](#licensing)

## Ways to contribute

- **Report a bug** — use the *Bug report* issue form.
- **Report TV/CEC compatibility** — tell us how Chalkboard behaves with your
  TV using the *TV/CEC compatibility* form. This feeds our compatibility
  matrix and is hugely valuable.
- **Suggest a feature** — use the *Feature request* form.
- **Propose a spec change** — use the *Spec change* form (see
  [Changing the specification](#changing-the-specification)).
- **Ask a question** — use GitHub Discussions, not issues.
- **Improve docs or fix bugs** — pull requests welcome.

## Before you start: issue first

| Change | Process |
|---|---|
| Typos, docs, small bug fixes, tests | Open a pull request directly. |
| New features, behaviour changes, refactors across modules, new dependencies | Open an issue first and wait until a maintainer marks it **accepted**. |
| Anything that changes `SPEC.md` | Spec change issue, then a `spec` pull request. |

This protects your time: the specification is still moving, and an accepted
issue means we agree on the *what* before you invest in the *how*. Comment on
an issue to claim it so work isn't duplicated.

## Developer Certificate of Origin

Every commit must be signed off to certify that you wrote it or otherwise have
the right to submit it under the project's license, per the
[Developer Certificate of Origin 1.1](https://developercertificate.org/).

```bash
git commit -s
```

This adds a trailer matching your git identity:

```text
Signed-off-by: Your Name <you@example.com>
```

A DCO check runs on every pull request and blocks merging if any commit is
missing a sign-off. To fix the last commit, use `git commit --amend -s`; for
several, `git rebase --signoff main`.

## Branches and pull requests

- `main` is the only long-lived branch. It is protected: changes land through
  pull requests with passing CI and at least one maintainer approval.
- Create a short-lived branch from `main` named `<type>/<short-description>`,
  e.g. `feat/cec-watchdog`, `fix/sync-tombstone-gc`, `docs/readme-hardware`.
- Keep pull requests focused: one logical change per PR. Large work should be
  split into reviewable steps.
- Pull requests are **squash-merged**. The PR title becomes the commit message
  on `main`, so **the PR title must follow the commit convention below**.
- Fill in the pull request template checklist.
- Rebase on `main` if your branch falls behind or conflicts.
- Releases are tagged `vX.Y.Z` on `main` following
  [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
with a scope:

```text
<type>(<scope>): <summary in imperative mood, lower case, no period>

<optional body: what and why>

Refs: SYNC-004, #123
Signed-off-by: Your Name <you@example.com>
```

**Types:** `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`,
`chore`, `revert`.

**Scopes:**

| Scope | Area |
|---|---|
| `render` | `chalkboard-render`, LVGL, DRM/KMS, WASM preview backend |
| `tpl` | Template schema and parser |
| `agent` | `chalkboard-agent` core, process supervision, IPC |
| `sync` | Discovery, membership, replication, HLC |
| `net` | Wi-Fi, fallback AP, Ethernet |
| `cec` | HDMI-CEC |
| `time` | NTP, RTC, peer time, scheduling |
| `web` | Web app (Svelte) and HTTP API |
| `sec` | Authentication, certificates, keys |
| `prov` | Provisioning and setup |
| `upd` | Firmware updates |
| `os` | Buildroot tree, kernel, boot |
| `spec` | `SPEC.md` |
| `docs` | Other documentation |
| `ci` | CI workflows and tooling |

- Mark breaking changes with `!` after the scope (`feat(sync)!: …`) and a
  `BREAKING CHANGE:` footer describing the migration.
- Reference spec requirement IDs and issues in a `Refs:` footer when relevant.

Examples:

```text
feat(cec): re-assert active source when input is switched away

fix(sync): keep tombstones until all nodes have acknowledged them

Refs: SYNC-007
```

## Changelog

We keep a human-written [CHANGELOG.md](CHANGELOG.md) following
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

- Every pull request with a **user-visible effect** must add an entry under
  `## [Unreleased]` in the matching section: `Added`, `Changed`, `Deprecated`,
  `Removed`, `Fixed` or `Security`.
- Write for people who install and run Chalkboard, not as a commit log:
  describe the effect, not the implementation.
- Purely internal changes (CI, refactors, tests, tooling) are exempt; a
  maintainer applies the `skip-changelog` label.
- CI fails if a non-exempt PR does not touch `CHANGELOG.md`.

## Code style and checks

All formatting is automated and enforced in CI. Warnings are errors.

| Language | Tools |
|---|---|
| Rust | `rustfmt`, `clippy -D warnings` |
| C | `clang-format`, `clang-tidy`, compiled with `-Wall -Wextra -Werror` |
| Web (Svelte/TS) | Prettier, ESLint, `svelte-check` |
| Shell | `shellcheck` |
| Markdown | `markdownlint` |

Run everything CI runs, locally, with [`just`](https://github.com/casey/just):

```bash
just check
```

`just fmt` applies all formatters. An optional pre-commit hook can be
installed with `just install-hooks`.

Prerequisites for `just check`:

- [rustup](https://rustup.rs/) (the toolchain version is pinned in
  `rust-toolchain.toml`)
- CMake ≥ 3.20 and a C11 compiler
- LLVM's `clang-format` and `clang-tidy` (macOS: `brew install llvm`)
- `shellcheck`
- Node.js with [pnpm](https://pnpm.io/) (version pinned in `package.json`);
  JavaScript tools, including markdownlint, are installed from the lockfile
- [`just`](https://github.com/casey/just)

The repository layout is described in
[ADR 0001](docs/adr/0001-monorepo-layout.md).

Every source file must start with an SPDX license header, for example:

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
```

## Testing

Chalkboard's core promises — fast boot, power-loss safety, offline operation —
must be protected by tests.

- **Every behaviour change needs automated tests that run without hardware**:
  unit tests, property-based tests, golden-image tests using the headless/WASM
  renderer, and the multi-node sync simulator.
- Name or annotate tests with the requirement IDs they verify where applicable
  (e.g. `sync_007_tombstones_retained`).
- **Hardware-sensitive changes** — boot path, DRM/KMS output, CEC, Wi-Fi and
  fallback AP, storage and power-loss handling, updates — must describe in the
  PR how they were tested on real hardware (if at all) and get the
  `needs-hil` label. A maintainer runs the hardware-in-the-loop suite (boot
  time, power-cut tests, rollback) before merging.
- Changes that may affect performance budgets (SPEC §21) should include
  before/after measurements.

You don't need a Raspberry Pi to contribute — but if you have one and a TV,
CEC compatibility reports are always welcome.

## Changing the specification

[SPEC.md](SPEC.md) is the single source of truth. Code must never silently
contradict it.

1. Open a *Spec change* issue describing the problem and the proposed
   requirement changes.
2. Once accepted, open a pull request labelled `spec` with a `docs(spec):`
   (or `feat(spec)!:` for breaking) title.
3. Follow the spec's own rules:
   - requirement IDs are never reused or renumbered;
   - removed requirements are kept and marked *Withdrawn*;
   - bump the spec version: MAJOR for removed or incompatible requirements,
     MINOR for added requirements, PATCH for clarifications.
4. A code change that needs a spec change must include it in the same pull
   request or follow an already merged spec PR.

## AI-assisted contributions

AI tools are welcome, with the same quality bar as any other contribution:

- **You are responsible** for every line you submit. You must understand it,
  be able to explain it in review, and have tested it.
- Your **DCO sign-off applies** in full: you certify you have the right to
  submit the code.
- **Disclose substantial AI assistance** with a commit trailer such as
  `Assisted-by: <tool name>` or a note in the pull request description.
- Low-effort, unreviewed or bulk-generated pull requests will be closed
  without detailed review.

## Licensing

Chalkboard is dual-licensed under [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE). Unless you explicitly state otherwise, any
contribution intentionally submitted for inclusion in the work by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.

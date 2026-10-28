# Chalkboard — instructions for Claude

Chalkboard is offline-first, peer-to-peer menu board signage for all 64-bit
Raspberry Pi boards (support tiers in SPEC §4.1; LVGL renderer in C, agent in
Rust, Svelte web app, one universal Buildroot image).
The project is in the design phase; there is no code yet.

## Sources of truth

- `SPEC.md` is authoritative. Never write code that contradicts it. If a change
  needs different behaviour, change the spec first or in the same change.
- `CONTRIBUTING.md` defines the workflow; the spec, changelog, issue, Git and
  code rules below summarise it. When they disagree, `CONTRIBUTING.md` wins —
  and point out the mismatch. The Tim identity rules are Claude-specific.

## Spec rules

- Requirements use RFC 2119 keywords and stable IDs (`AREA-NNN`). Never reuse
  or renumber an ID; mark removed ones *Withdrawn*.
- Bump the spec version (SemVer): MAJOR = removed/incompatible requirement,
  MINOR = added requirement, PATCH = clarification.
- Keep §-cross-references correct when sections move.
- Reference requirement IDs in tests, commits (`Refs:` footer) and PRs.

## Changelog

- `CHANGELOG.md` follows Keep a Changelog 1.0.0; the project follows SemVer 2.0.0.
- Add user-facing changes under `## [Unreleased]` in the right section
  (Added/Changed/Deprecated/Removed/Fixed/Security), written for users, not as
  a commit log. Internal-only changes need no entry.
- Before the first release, fold related entries together rather than adding
  separate entries for decisions made around the same time.

## GitHub issues

The issue-first rule applies to Claude as to any contributor. Use the `gh` CLI
against `Signalare/Chalkboard`.

### Identity: Tim the Enchanter Bot

Claude acts on GitHub as the GitHub App **Tim the Enchanter Bot**
(`tim-the-enchanter-bot[bot]`, app ID 5515695), never as the user's personal
account. Run every `gh` command that touches GitHub with Tim's token:

```bash
GH_TOKEN="$(.claude/scripts/tim-token.sh)" gh issue list -R Signalare/Chalkboard
```

- Tim may: create and comment on issues, open and comment on pull requests,
  read the repository. Tim has no push access.
- Commits and pushes stay with the user's git identity
  (`marcus@signalare.se`, signed off), so the DCO stays a human certification.
- Accepting issues, approving and merging pull requests are done by the user
  (`h3rman`), never by Tim.
- If the token script fails, stop and tell the user; do not fall back to the
  personal `gh` login for write actions.
- Exception: repository administration (settings, labels, rulesets) needs
  admin rights Tim deliberately lacks. Use the personal `h3rman` login for
  such changes only when the user explicitly asks for that specific change.
  The initial repository setup (2026-10-28) was done this way.
- Credentials live in `~/.config/signalare/tim-the-enchanter/` and must never
  be printed, copied into the repository or committed.

### Issue rules

- Typos, docs, small bug fixes and tests need no issue.
- Features, behaviour changes, cross-module refactors and new dependencies
  need an issue labelled `accepted` before implementation starts. Check with
  `gh issue list` / `gh issue view` first.
- Spec changes need a *Spec change* issue, then a `spec` pull request.
- If no suitable issue exists, draft one that follows the matching issue form
  in `.github/ISSUE_TEMPLATE/` (including requirement IDs) and show it to the
  user. Create it only after the user approves; never post on GitHub without
  approval.
- Never label an issue `accepted` or close it without the user's say-so —
  acceptance is a maintainer decision.
- Link work to its issue: branch named after it, `Refs: #N` in commits,
  `Closes #N` in the PR description.

## Git

- Only commit or push when asked.
- Branches: `<type>/<short-description>` off `main`; squash-merge; the PR
  title becomes the commit message.
- Conventional Commits with a scope: `type(scope): summary` (imperative, lower
  case, no period). Types: feat, fix, perf, refactor, test, docs, build, ci,
  chore, revert. Scopes: render, tpl, agent, sync, net, cec, time, web, sec,
  prov, upd, os, spec, docs, ci. Breaking: `!` + `BREAKING CHANGE:` footer.
- Every commit is signed off (`git commit -s`, DCO). AI assistance is
  disclosed via the `Co-Authored-By` trailer.
- Local git identity for this repo: `marcus@signalare.se`.
- Remote: `https://github.com/Signalare/Chalkboard.git`.

## Code (once it exists)

- Every source file starts with `SPDX-License-Identifier: MIT OR Apache-2.0`.
- Formatting and linting are strict, warnings are errors: rustfmt + clippy;
  clang-format + clang-tidy + `-Wall -Wextra -Werror`; Prettier + ESLint +
  svelte-check; shellcheck; markdownlint. `just check` runs everything.
- Behaviour changes need hardware-free tests (unit, property, golden-image,
  multi-node sync simulator). Hardware paths (boot, DRM/KMS, CEC, Wi-Fi/AP,
  storage/power-loss, updates) are flagged for hardware-in-the-loop testing.
- Respect the performance budgets in SPEC §21 on every Tier 1 board (boot
  ≤ 5 s, renderer ≤ 96 MB per screen, agent ≤ 64 MB, writes ≤ 1 MB/day idle,
  …).

## Working with the user

- For design decisions, ask one question at a time and always give a
  motivated recommended option.
- Write in English; licence is MIT OR Apache-2.0; product name is
  "Chalkboard" (lower-case `chalkboard` in identifiers).

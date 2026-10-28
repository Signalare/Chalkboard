# 1. Monorepo layout

- **Status:** Accepted
- **Date:** 2026-10-28
- **Issue:** [#1](https://github.com/Signalare/Chalkboard/issues/1)

## Context

Chalkboard consists of a C renderer, a Rust agent, a Svelte web app, shared
schemas and a Buildroot OS image (SPEC §3, §5). Many changes cut across
components: a template change touches the schema, the renderer and the web
app at once.

## Decision

One repository with one top-level folder per component:

| Folder | Contents | Commit scope |
|---|---|---|
| `agent/` | Rust workspace for `chalkboard-agent` and its crates | `agent`, `sync`, `net`, `cec`, `time`, `sec`, `prov`, `upd` |
| `render/` | C renderer (CMake): core, `backends/drm`, `backends/wasm` | `render` |
| `web/` | Svelte web app (pnpm) | `web` |
| `schema/` | JSON Schemas shared by all components | `tpl` |
| `os/` | Buildroot external tree (`BR2_EXTERNAL`) | `os` |
| `tools/` | Developer scripts, write tracer, HIL harness | `ci` |
| `tests/` | Cross-component tests | `ci` |
| `docs/` | Design notes, spike reports, ADRs | `docs` |

- Buildroot is downloaded at a pinned version, not vendored as a submodule.
- The repository root is a pnpm workspace so JavaScript tooling (including
  markdownlint) is pinned once for everyone.
- `just check` is the single entry point that mirrors CI.

## Consequences

- Cross-cutting changes land in one pull request and are reviewed together.
- Each component keeps its native build tool (Cargo, CMake, pnpm, Buildroot),
  orchestrated by `just`.
- CI must be path-aware later to avoid rebuilding everything on every change.

## Alternatives considered

- **Separate repositories per component** — cleaner boundaries, but
  cross-cutting changes would need coordinated pull requests.
- **Buildroot as a git submodule** — vendored and offline-friendly, but heavy
  to clone and awkward to bump.

# Hawk Rename Design

**Date:** 2026-02-28  
**Status:** Approved  
**Repo:** https://github.com/darkwingrick/hawk

## Background

Hawk is an open-source fork of [Zed](https://github.com/zed-industries/zed), a high-performance code editor. This document describes the strategy for safely and legally renaming the application from "Hawk" to "Hawk" while maintaining full AGPL-3.0 compliance.

---

## Legal Strategy

### Obligations (AGPL-3.0)

- **Preserve**: All `LICENSE-AGPL`, `LICENSE-GPL`, `LICENSE-APACHE` files verbatim
- **Preserve**: `Copyright 2022-2025 Zed Industries, Inc.` notices — cannot be removed
- **Add**: `Copyright 2026 darkwingrick` in a top-level `NOTICE` file
- **Add**: Attribution in `README.md`: _"Hawk is based on [Zed](https://github.com/zed-industries/zed), © Zed Industries, Inc."_

### Files to Rewrite (Hawk-specific content)

- `README.md` — full rewrite for Hawk
- `CONTRIBUTING.md` — full rewrite
- `CODE_OF_CONDUCT.md` — adopt Contributor Covenant
- `legal/terms.md` — rewrite for Hawk
- `legal/privacy-policy.md` — rewrite for Hawk
- `legal/third-party-terms.md` — rewrite for Hawk
- `legal/subprocessors.md` — rewrite for Hawk
- Add: `NOTICE` file with dual copyright attribution

---

## Cloud Feature Strategy

### Remove Outright

| Component                                     | Action                                         |
| --------------------------------------------- | ---------------------------------------------- |
| Crash reporting (`crashes` crate)             | Delete integration points; stub crate remains  |
| Telemetry upload endpoint (`telemetry` crate) | Wipe upload URL; keep struct for compilability |

### Feature-Flagged OFF by Default

| Feature            | Crate(s)                                       | UI Behavior When OFF                        |
| ------------------ | ---------------------------------------------- | ------------------------------------------- |
| Multiplayer collab | `collab`, `collab_ui`, `call`, `channel`       | Sign-in button and collab panel hidden      |
| Remote dev servers | `remote`, `remote_server`, `remote_connection` | Menu items hidden; message pointing to docs |
| Google Auth        | `client` auth flow                             | No sign-in prompt at startup                |

### Re-pointed (Keep, Different Target)

| Service            | Old URL                | New URL                                                 |
| ------------------ | ---------------------- | ------------------------------------------------------- |
| Auto-update        | `api.hawk.dev/releases` | `api.github.com/repos/darkwingrick/hawk/releases`       |
| Extension registry | `hawk.dev/extensions`   | Keep Zed's registry (WASM-compatible)                   |
| Docs links         | `hawk.dev/docs/*`       | `https://github.com/darkwingrick/hawk/tree/master/docs` |

> **Extension registry note**: Hawk is binary-compatible with Zed extensions (same WASM host). Pointing at Zed's public registry gives users immediate full extension access with no infrastructure cost. Can be replaced with a self-hosted registry later.

---

## Core Rename Mechanics

### Crate Renames

| Old            | New             | Location                                         |
| -------------- | --------------- | ------------------------------------------------ |
| `zed`          | `hawk`          | `crates/zed/` → `crates/hawk/`                   |
| `zed_actions`  | `hawk_actions`  | `crates/zed_actions/` → `crates/hawk_actions/`   |
| `zed_env_vars` | `hawk_env_vars` | `crates/zed_env_vars/` → `crates/hawk_env_vars/` |

After renames, update the workspace `Cargo.toml` member list and all `[dependencies]` sections that reference these crates by name.

### Binary & Metadata

- Binary name: `zed` → `hawk` (`default-run` in `Cargo.toml`)
- Bundle ID: `dev.zed.Zed` → `com.darkwingrick.hawk` (`.plist`, build scripts)
- Cargo authors: `"Zed Team <hi@hawk.dev>"` → `"darkwingrick"`

### User-Facing Strings

Targeted replacement of UI strings only (not Rust identifiers):

- `"Hawk"` → `"Hawk"` in display strings, window titles, notifications
- `"hawk.dev"` in user-visible messages → `"github.com/darkwingrick/hawk"`

### Config Paths

In the `paths` crate — all platform config/data/log paths:

- `~/.config/zed/` → `~/.config/hawk/`
- `~/.local/share/zed/` → `~/.local/share/hawk/` (Linux)
- `~/Library/Application Support/Zed/` → `~/Library/Application Support/Hawk/` (macOS)

### Environment Variables

In `hawk_env_vars` (formerly `zed_env_vars`):

- All `ZED_*` vars → `HAWK_*` (e.g. `ZED_STATELESS` → `HAWK_STATELESS`)

---

## Execution Order

Phases must be done sequentially with a `cargo check` between each to avoid an uncompilable mid-rename state.

### Phase 1 — Crate Renames _(highest risk)_

1. Rename `crates/zed/` → `crates/hawk/` (directory + `Cargo.toml` name field)
2. Rename `crates/zed_actions/` → `crates/hawk_actions/`
3. Rename `crates/zed_env_vars/` → `crates/hawk_env_vars/`
4. Update workspace `Cargo.toml` members list
5. Update all dependency references across all crates
6. **Audit the existing partial rename** — `main.rs` already shows `n` references (something was started); understand and reconcile before continuing
7. `cargo check` — must compile cleanly before Phase 2

### Phase 2 — Feature Flagging _(isolate cloud code)_

1. Feature-flag off collab, auth, remote servers in relevant crates
2. Delete crash reporting integration points
3. Wipe telemetry upload URL
4. Re-point auto-update URL to GitHub releases API
5. Re-point all doc links to `https://github.com/darkwingrick/hawk/tree/master/docs`
6. `cargo check`

### Phase 3 — String & Metadata Pass _(cosmetic, low risk)_

1. User-facing `"Hawk"` → `"Hawk"` strings
2. Bundle ID, plist, Cargo metadata
3. Config paths in `paths` crate
4. `ZED_*` env vars → `HAWK_*`
5. Update `typos.toml` allowlist (has `zed` as allowed word)
6. `cargo check`

### Phase 4 — Docs & Legal _(no build impact)_

1. Add `NOTICE` file
2. Rewrite `README.md`
3. Rewrite `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`
4. Rewrite `legal/` docs

---

## Known Risks

| Risk                                                             | Mitigation                                         |
| ---------------------------------------------------------------- | -------------------------------------------------- |
| Partial rename already in progress (`n` references in `main.rs`) | Audit first; reconcile or clean up before starting |
| Naive global find/replace corrupts Rust internals                | Use category-based passes, not global substitution |
| `typos.toml` allowlist has `zed`                                 | Update in Phase 3                                  |
| `Cargo.lock` regeneration                                        | Expected; not risky                                |
| macOS notarization scope change                                  | Document for when distribution is set up           |
| Extension registry compatibility                                 | Monitor Zed API for breaking changes               |

---

## Future Work (Out of Scope for Initial Rename)

- Google OAuth integration (requires Google Cloud project + credentials)
- Self-hosted extension registry
- Self-hosted collab server
- Remote dev server support
- Custom docs site (for now: GitHub tree view)

# Rust Dependency Upgrade Plan

## Scope

Target the ecosystem bumps surfaced by `cargo upgrade --dry-run --incompatible`:

- `axum` `0.7.x → 0.8.6` (see `axum/CHANGELOG.md`).
- `tower-http` `0.5.x → 0.6.6` (see `tower-http/CHANGELOG.md`).
- `tokio` `1.0 → 1.48.0`.
- `ratatui` `0.28.x → 0.29.0` (plus associated `crossterm 0.29` and `unicode-width 0.2.2`, ref. `ratatui/CHANGELOG.md`, `BREAKING-CHANGES.md`, `crossterm/CHANGELOG.md`).
- `toml` `0.8.x → 0.9.8` (ref. `crates/toml/CHANGELOG.md`).
- `dialoguer` `0.11.0 → 0.12.0` (GitHub release notes `v0.12.0`).
- `rand` `0.8.5 → 0.9.0` (GitHub release notes `0.9.0`).
- `md5` `0.7.x → 0.8.0` (RustCrypto `md5/CHANGELOG.md`).

Patch-level bumps (e.g., `clap 4.5.42 → 4.5.49`, `serde 1.0.219 → 1.0.228`, `chrono 0.4.41 → 0.4.42`) will be folded in opportunistically during execution.

## Baseline Snapshot

1. Record current behaviour: `cargo fmt --check`, `cargo test --all`, `cargo clippy --workspace` (if already part of CI).
2. Build frontend assets: `pnpm install` (if needed) then `pnpm run build:frontend`.
3. Per project process doc, spin up the tmux-based dev stack (`pnpm run dev:ui`) once to confirm current UI/API traces before making changes; capture log snippets for comparison.

## Upgrade Stages

### Stage 1 – Low-Risk Patch Updates

1. Run `cargo upgrade` (without `--incompatible`) to land all compatible bumps.
2. Re-run the baseline test/build suite to ensure no regressions before tackling major versions.

### Stage 2 – Configuration Stack (`toml` Family)

1. Update `toml` to `0.9.x`, explicitly enabling `features = ["serde", "std"]` if we rely on the defaults that were split out.
2. Audit serialization/deserialization touchpoints for the new buffer-based serializer signatures and `preserve_order` implications.
3. Re-run configuration-specific tests (parsers, fixtures) plus `cargo test --lib`.

### Stage 3 – HTTP Server Stack

1. Bump `axum` to `0.8.6`, ensuring route paths adopt the `{param}` syntax and that handlers satisfy the stricter `Send + Sync` bounds.
2. Upgrade `tower-http` to `0.6.6`; verify the `normalize-path`, `fs`, and `cors` features still align with usage (enable additional feature flags if the trimmed `body` module is required).
3. Keep `tokio` at the latest `1.48.0` to match axum’s MSRV expectations; audit runtime builder usage for new defaults.
4. Execute targeted HTTP endpoint tests (unit tests plus `curl` smoke via the dev server).

### Stage 4 – TUI Stack (`ratatui`, `crossterm`, `unicode-width`)

1. Upgrade `ratatui` to `0.29.0`, walking through API changes (`Block::title`, `Line` conversions, widget module moves).
2. Align `crossterm` (`0.29.0`) and `unicode-width` (`0.2.2`) to satisfy the new dependency tree.
3. Validate terminal UI flows with existing integration tests and manual runs if applicable.

### Stage 5 – CLI & Utility Crates

1. Update `dialoguer` to `0.12.0`; adapt prompts to iterator-based inputs where necessary.
2. Move `rand` to `0.9.0`, updating renamed APIs (`thread_rng` → `rng`, `gen_range` → `random_range`, etc.) and feature flags.
3. Bump `md5` (or the encompassing `RustCrypto::md5`) to `0.8.0`, confirming hashing call-sites build cleanly.

## Verification & Sign-Off

1. Full workspace checks: `cargo fmt`, `cargo clippy --workspace --all-targets`, `cargo test --workspace --all-features`.
2. Frontend build (`pnpm run build:frontend`) and UI smoke (`pnpm run dev:ui` via tmux, validate logs and key interactions).
3. Document observed regressions or follow-up tasks; if blockers surface, park the offending upgrade in its own branch with findings.
4. Once stable, capture the change summary and reference the relevant changelog links in commit/PR descriptions for reviewer context.

## Contingencies

- If a major upgrade proves too disruptive, isolate it (e.g., leave `ratatui` pinned) and file an issue noting required follow-up work.
- Track MSRV implications (Axum 0.8 requires Rust ≥1.78; Ratatui 0.29 + Crossterm 0.29 imply Rust 1.70+; adjust CI toolchains if needed).
- Watch for transitive dependency conflicts (`unicode-width` is a known constraint between `ratatui` and direct usage—avoid downgrading manually).

## References

- `axum/CHANGELOG.md`, `tower-http/CHANGELOG.md`.
- `ratatui/CHANGELOG.md`, `BREAKING-CHANGES.md`, `crossterm/CHANGELOG.md`.
- `crates/toml/CHANGELOG.md`.
- GitHub releases: `console-rs/dialoguer@v0.12.0`, `rust-random/rand@0.9.0`.
- `RustCrypto/hashes/md5/CHANGELOG.md`.

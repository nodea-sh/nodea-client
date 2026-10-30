# CLAUDE.md

Guidance for working in `nodea-client` — the standalone Nodea desktop app
(Tauri + React frontend in `src/`, Rust backend in `src-tauri/`). Merge with
any user-level instructions.

## Incremental flow and testing

- Write unit tests for new code where applicable.
- Cover happy paths with integration tests where applicable.
- Ask clarifying questions before jumping into implementation.
- Work in incremental, independently shippable steps.

## Authentication

The frontend always passes authentication tokens via headers (see
`src/lib/api.ts`).

## Testing before commit

Run the tests for the code you touched before committing, and distinguish what
you ran locally from what CI runs:

- **Rust (`src-tauri/`)** — `make test-rust` (or `cargo test`). CI runs this in
  the `rust` job of `.github/workflows/ci.yml`.
- **Frontend (`src/`)** — the frontend end-to-end suite is Playwright
  (`npm test`, specs in `tests/`, `testDir: ./tests`). CI runs this in the
  `e2e` job. The `frontend` job type-checks and builds (`tsc && vite build`).
- When a change spans both surfaces (e.g. a Rust command the frontend calls),
  run both — a type-check pass does not prove the runtime wiring works.

Other CI jobs in `ci.yml`: `scripts` (pytest over `scripts/tests/`),
`licenses`, `fmt`, and `clippy`. A suite CI does not run is decorative — if you
add one, wire it into `ci.yml` in the same PR.

## mistralrs / mistralrs-metal-compile (Metal release build)

`mistralrs` is a git dependency pinned to a release tag (0.9 is not published
to crates.io). `src-tauri/vendor/mistralrs-metal-compile-<version>/` is a
vendored, patched copy of the `mistralrs-metal-compile` crate, wired in via
`[patch."https://github.com/EricLBuehler/mistral.rs"]` in
`src-tauri/Cargo.toml`. This is deliberate — do not remove it or "clean up" the
vendor directory without reading this section.

**Why it's vendored:** `mistralrs-metal-compile` builds the Metal (GPU) kernels
for `mistralrs-core`, `mistralrs-quant` and `mistralrs-paged-attn`. Under a
**production release build**'s heavier concurrent load (`npm run tauri build` /
`cargo build --release`, never `cargo test` or `tauri dev`), its
`.air -> .metallib` link subprocess can get resource-starved and produce a
near-empty `.metallib` while still exiting 0. That leaves the model with no
working GPU code (symptom: every chat response comes back empty, with
`Error while loading function: fused_glu_float` or similar in the logs). The
vendored copy adds a verify-and-retry safety net (checks every compiled
`.metallib` defines kernel symbols via `metal-nm`, retries, and fails the build
loudly instead of shipping a broken one). Full investigation and evidence: PR
#161 in the upstream `ai4all-io/nodea` repo. A second bug that needed patching
in 0.8 (the link step passing no `--sdk`/`-std` flags) is fixed upstream as of
0.9.

`src-tauri/.cargo/config.toml` sets `MISTRALRS_METAL_PLATFORMS=macos` so only
the macOS metallibs are built — this is a macOS-only app for Metal.

**macOS 15 minimum:** 0.9's candle uses `MTLResidencySet`, which does not exist
on macOS 14, so `tauri.conf.json` sets `minimumSystemVersion` to `15.0`.
Re-check this before any bump.

**If you bump the `mistralrs` version:**

1. Update the tag in both `mistralrs` entries in `src-tauri/Cargo.toml`.
2. Copy the new tag's `mistralrs-metal-compile/` into
   `src-tauri/vendor/mistralrs-metal-compile-<version>/`, replacing its
   `Cargo.toml` workspace-inherited fields with concrete values from the
   upstream workspace `Cargo.toml` (including the candle git rev).
3. Re-apply the `NODEA PATCH` changes in `src/lib.rs` (the
   `compile_platform_verified` wrapper and `metallib_has_symbols`) and update
   `PATCHES.md`.
4. Update the path in the `[patch."https://github.com/EricLBuehler/mistral.rs"]`
   section.
5. **Verify on a real production build before merging** — `npm run tauri build`,
   then run an actual chat message through the built `.app`. `cargo test` and
   `tauri dev` cannot catch the bug.
6. If a future version fixes this upstream, confirm with a real release-build
   test first, then the vendor directory and patch entry can be deleted.

## No extra documentation by default

Generate only what was asked. Do not create README, docs, or explanation files
unless explicitly requested.

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

## Inference (llama.cpp)

All model logic is in `src-tauri/crates/nodea-inference` (no Tauri, no
`.env.local`); the app's `ModelManager` only wraps it. The design and the proto
contract it must keep are in `notes/llama-cpp-4-migration.md`.

- **Iterate without the app.** `make fetch-test-models` (pinned GGUFs, about
  1.5 GB), then `make test-inference` for the contract suite, or
  `make infer MODEL=tools REQ=crates/nodea-inference/examples/requests/tools-weather.json`
  to see the exact `GeneratedText` messages (`ARGS=--show-prompt` shows the
  rendered template and grammar). CI runs the suite in the `inference` job.
- **Every new behaviour needs a contract test** in
  `crates/nodea-inference/tests/contract_*.rs` that calls
  `support::assert_invariants`, and a new capability needs a row in
  `support::CAPABILITY_TESTS`.
- **`llama-cpp-4` is pinned exactly** (`=0.11.0`) because its API breaks between
  minor versions. Bump it deliberately, behind the contract suite, together
  with `llama-cpp-sys-4`. Never enable its `prebuilt` (downloads binaries at
  build time) or `native` (bakes the build CPU into the binary) features.
- **Templates and tool calls are llama.cpp's** (`llama_cpp_4::chat`). Do not
  parse model-specific markers in our code.
- **GPU backends:** macOS always builds Metal (shaders embedded in the binary);
  Windows and Linux release builds pass `--features vulkan`. Dev and CI builds
  elsewhere are CPU only. A real GPU run is the manual release checklist in
  the spec (§8.8): `cargo test` cannot cover it.

## No extra documentation by default

Generate only what was asked. Do not create README, docs, or explanation files
unless explicitly requested.

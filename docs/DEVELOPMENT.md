# Developing Nodea Desktop

How to build, run, test and release the desktop app from source. For what the
app is and how to install it, see the [README](../README.md).

The Nodea backend is a hosted service and is not part of this repository.
Without a Nodea account you can build the app and explore its UI (see
[tests/README.md](../tests/README.md)), but the node will not serve anything.

## Architecture

The app is built with:
- **Backend**: Rust (Tauri) with tonic for gRPC and llama.cpp (`llama-cpp-4`) for local inference, in `src-tauri/crates/nodea-inference`
- **Frontend**: React with TypeScript, TailwindCSS, and shadcn/ui components
- **Authentication**: Supabase with JWT (RS256)
- **Protocol**: Protocol Buffers (see `protos/service.proto`)

### Key Components

1. **src-tauri/src/auth.rs**: Supabase authentication with email/password and OAuth
2. **src-tauri/src/grpc_client.rs**: gRPC client with bidirectional streaming
3. **src-tauri/src/main.rs**: Tauri commands and state management
4. **src/**: React frontend with routing, API client, and UI components

## Prerequisites

- Rust 1.88+ (`llama-cpp-4`'s minimum)
- CMake and clang/libclang: llama.cpp is built from source. Release builds for
  Windows and Linux also need the Vulkan SDK (`glslc`), for `--features vulkan`.
- Node.js 20+
- On Linux, the Tauri v2 system packages: `libgtk-3-dev libwebkit2gtk-4.1-dev
  libayatana-appindicator3-dev libxdo-dev librsvg2-dev patchelf libssl-dev`.
  CI and the release AppImage build on Ubuntu 22.04, in a container.

Supported platforms: macOS on Apple Silicon is the primary target, and its
releases are signed and notarised. Linux (AppImage) and Windows (MSI)
builds use Vulkan, which covers NVIDIA, AMD and Intel GPUs with no runtime to
bundle, and fall back to the CPU; they are unsigned and less tested.

`protoc` is **not** a prerequisite: `src-tauri/crates/nodea-proto/build.rs` compiles the proto with
a vendored binary (`protoc-bin-vendored`). See [docs/PROTO_SYNC.md](PROTO_SYNC.md).

## Quick Start

```bash
npm install

# src-tauri/src/auth.rs reads SUPABASE_URL, SUPABASE_ANON_KEY and
# VITE_API_BASE_URL through the env!() macro, so the Rust crate does not
# compile at all until one of the .env files is in place.
cp .env.example .env.local

npm run tauri:local
```

`npm run dev` alone starts only the Vite dev server (the UI in a browser, in
demo mode), not the desktop app.

### Environment Configuration

Create `.env.local` for local development against a local backend:

```bash
cp .env.example .env.local
```

The defaults target a local stack:
- `GRPC_SERVER=http://localhost:50051`
- `SUPABASE_URL` / `VITE_SUPABASE_URL` and the matching anon keys point at a
  local Supabase CLI stack (`http://127.0.0.1:54321`, with the CLI's public demo
  key)

To point the app at a shared remote backend instead, edit `.env.development` with its host.


## Running the App

### Development Mode

```bash
npm run tauri:local  # local backend (default: `npm run tauri dev` also uses this)
npm run tauri:dev    # shared remote backend
```

This starts the Vite dev server and launches the Tauri app with hot-reload.

### Production Build

```bash
npm run build
npm run tauri build
```

The built app will be in `src-tauri/target/release/bundle/`.

### Releases

Releases are versioned `YY.M.X`: the year, the month, and a counter for that
month (`26.10.1`, `26.10.2`, then `26.11.1`). Running the **Release Desktop
App** workflow on `main` tags the next version, builds macOS, Linux and Windows
from that tag, and creates a draft GitHub Release with the bundles, a
`SHA256SUMS` file and build-provenance attestations. Publishing the draft makes
it the download at `releases/latest`. The version in `package.json`,
`tauri.conf.json` and `Cargo.toml` is a placeholder; the release build stamps
the real one in.

#### Verifying a download

Each release includes a `SHA256SUMS` file and build-provenance attestations.
To check a download came from this repository:

```bash
gh attestation verify Nodea-mac.dmg --repo nodea-sh/nodea-client
```

## Testing

### Unit Tests (Rust)

```bash
cd src-tauri
cargo test
```

### Integration Tests (Playwright)

```bash
# Run tests headless
npm test

# Run with browser visible
npm run test:headed

# Open Playwright UI
npm run test:ui
```

## Development Notes

### Authentication Flow
- Supabase handles email/password and OAuth
- The Rust side keeps the access and refresh tokens in
  `<app data>/.auth_tokens.json`. When the UI runs in a plain browser instead
  (`npm run dev`), the Supabase JS client keeps its session in `localStorage`
- Tokens sent in HTTP headers and gRPC metadata
- Backend verifies using Supabase JWKS endpoint

### gRPC Connection
- Bidirectional streaming for device communication
- Automatic reconnection on disconnect
- Heartbeat messages maintain connection
- Status changes reflected in UI

### Local AI Inference
- Uses llama.cpp for local model execution (`src-tauri/crates/nodea-inference`,
  testable without the app: `make fetch-test-models && make test-inference`)
- Metal on macOS, Vulkan on Windows and Linux, CPU fallback
- Model download and caching
- Progress tracking during download

## Project Structure

```
.
├── src/                    # React frontend
│   ├── components/        # UI components
│   ├── lib/               # API client and utilities
│   ├── pages/             # Route pages
│   └── App.tsx            # Main app component
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── main.rs        # Entry point, Tauri commands and state
│   │   ├── lib.rs         # Library crate shared with tests
│   │   ├── auth.rs        # Supabase authentication
│   │   └── grpc_client.rs # gRPC streaming client
│   ├── crates/
│   │   ├── nodea-proto/     # Generated gRPC types (protos/service.proto)
│   │   └── nodea-inference/ # llama.cpp engine, request/wire mapping, contract tests
│   ├── build.rs           # Build script (compile-time env, Tauri)
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── tests/                 # Playwright tests - no backend required
├── protos/                # Vendored gRPC contract, see docs/PROTO_SYNC.md
│   └── service.proto
├── scripts/               # release job helpers (versioning, bundle selection) and their tests
├── .github/workflows/     # CI and the release matrix
├── package.json           # Node dependencies
└── vite.config.ts         # Vite configuration
```

## Related Documentation

- [Protobuf sync](PROTO_SYNC.md) - How the gRPC contract stays in step
- [Test setup](../tests/README.md) - Running the Playwright suite

## Third-party licences

The desktop bundle statically embeds its whole dependency tree - around 900 Rust
crates and 200 npm packages. It is almost entirely MIT and Apache-2.0, plus
BSD, ISC, Zlib, Unicode, CC0, 0BSD, OpenSSL, CDLA-Permissive-2.0 and a handful
of MPL-2.0 crates (`cssparser`, `selectors`, the `symphonia` codecs and a few
others). There is no GPL, AGPL, SSPL or BUSL. MPL-2.0 is file-level copyleft
and those files are unmodified, so it imposes nothing on this code.

Every licence text is in [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md),
which is bundled into every build next to LICENSE and NOTICE. It is generated by
`make licenses` (`scripts/generate-third-party-licenses.sh`: `cargo about` for
the crates, `license-checker` for the npm tree). The `licenses` CI job fails if
it is stale or if a dependency brings a licence outside the allowlist in
`src-tauri/about.toml`.

Models the app downloads are not part of this software and carry their own
licences.


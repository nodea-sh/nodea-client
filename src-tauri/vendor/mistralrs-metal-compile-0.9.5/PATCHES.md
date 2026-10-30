# Local modifications

This is `mistralrs-metal-compile` from the `v0.9.5` tag of
https://github.com/EricLBuehler/mistral.rs (MIT, Copyright (c) 2024 Eric
Buehler; see `LICENSE`), wired in through
`[patch."https://github.com/EricLBuehler/mistral.rs"]` in
`src-tauri/Cargo.toml`. `Cargo.toml` replaces the upstream workspace-inherited
fields with their v0.9.5 values; the only code change is in `src/lib.rs`,
marked with `NODEA PATCH` comments.

1. **Each compiled `.metallib` is verified and the link retried.** A metallib
   that defines no kernel symbols (as produced by a resource-starved link
   during a release build, while still exiting 0) is rejected, the compile is
   retried, and the build fails loudly if it never succeeds.

The two other fixes this app used to carry in its vendored `mistralrs-quant`
0.8.1 - checking the `.metal -> .air` exit status and passing `--sdk`/`-std`
to the `.air -> .metallib` link - are upstream as of 0.9. Building only the
macOS metallib is done through `MISTRALRS_METAL_PLATFORMS` in
`src-tauri/.cargo/config.toml` rather than a patch.

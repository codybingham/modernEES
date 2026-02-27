# Modern EES (working title)

Goal: Build an EES-like engineering equation solver desktop app:
- EES-like equation workflow (equations-first, auto unknown detection, Solve button)
- Modern UI (Tauri)
- Rust core engine
- CoolProp backend hidden behind a clean, EES-feeling property facade

This repo is built incrementally. See SPEC.md and ROADMAP.md.

## Development

Using `just`:

- `just fmt`
- `just clippy`
- `just test`

Or directly with Cargo:

- `cargo fmt --all`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`

## Tauri app (Task H)

Run the desktop app:

- `cargo run -p modern_ees_app`

If you have the Tauri CLI installed, you can also run:

- `cargo tauri dev --manifest-path app/Cargo.toml`

The app currently uses the core solver + mock properties backend, so CoolProp is not required for UI usage.

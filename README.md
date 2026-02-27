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

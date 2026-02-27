# modernEES app (Task H)

Minimal Tauri desktop shell wired to the `core` engine.

## Features

- Equations editor (textarea for V1)
- Analyze button (parse + unit diagnostics)
- Solve button
- Variables/results panel
- Param table sweeps + columns editor (JSON)
- Run Table button + results grid
- Basic plot (line + scatter) from selected X/Y columns
- Native menu bar (File, Edit, Calculate, Tables, Plots, Options, Help)

## Keyboard shortcuts

- `Ctrl+N` — New
- `Ctrl+O` — Open equations `.txt`
- `Ctrl+S` — Save equations `.txt`
- `Ctrl+Shift+S` — Save As
- `Ctrl+Q` — Quit
- `Ctrl+Enter` — Solve
- `F5` — Analyze
- `Ctrl+R` — Run Param Table

## Run locally

From repo root:

```bash
cargo run -p modern_ees_app
```

Or in dev mode with auto-reload for frontend edits:

```bash
cargo tauri dev --manifest-path app/Cargo.toml
```

> Note: this UI uses `MockPropsProvider` with fallback formula so it runs without CoolProp.

# SPEC — Modern EES-like App (Option 1)

## North Star
Replicate the core workflow and capabilities of EES as closely as practical, with a smoother modern UI.
CoolProp is allowed ONLY behind an internal property facade. Users must never write CoolProp-style calls.

## V1 Goals (must-haves)
1. Equations-first model editor:
   - Lines are equations/assignments, e.g. `m_dot = rho*A*V`
   - Functions like `sin(x)`, `log(x)`, etc.
   - Comments supported (`// ...` and `{ ... }`).
2. Variable discovery:
   - Detect variables and function calls.
   - Show known vs unknown state.
3. Units system:
   - Base units internally.
   - User-friendly display.
   - Dimensional consistency checking with clear diagnostics tied to equation line numbers.
4. Solver:
   - Automatic unknown selection (initial heuristic acceptable).
   - Robust nonlinear solve with damping/trust-region.
   - Scaling to reduce numerical issues.
   - Convergence report (iterations, residual norm, worst equations).
   - Persist last solution as default initial guess.
5. Thermo properties:
   - EES-like facade functions, e.g. `h(Water, T, P)`, `s(...)`, `rho(...)`, `T(Water, P, h)`.
   - Implemented via CoolProp, with aggressive caching.
6. Parametric table:
   - Sweep one or more input variables; compute outputs per row.
   - Reuse previous row solution as next initial guess.
7. Plot:
   - Simple X–Y plot from parametric table columns.
8. Project format:
   - Single `.eesx` file (zip): equations text + JSON settings + tables + plots.

## V1 Non-goals (explicitly NOT in V1)
- Optimization/goal seek
- Procedures/macros/includes
- Advanced plotting
- Multi-document workspaces
- REFPROP support

## Architecture constraints (mandatory)
- `core` crate: pure Rust library, no Tauri, no CoolProp, no Python.
- `props_coolprop` crate: implements property provider, caching, and unit normalization.
- `app` crate: Tauri UI only; must call into `core`.

## Quality bar
- Every task must include tests.
- `cargo test` must pass.
- Add or update docs whenever behavior changes.
- Prefer deterministic tests (fixed tolerances).

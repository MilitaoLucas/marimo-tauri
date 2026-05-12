# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

A Tauri 2 desktop wrapper around the [Marimo](https://marimo.io/) Python notebook server. There is no JavaScript frontend build step — the app spawns a local Marimo server via `uv run marimo edit ...` and points a WebviewWindow at `http://localhost:2730`. All application logic is pure Rust in `src/lib.rs`.

## Commands

### Development
```bash
cargo tauri dev          # build + run with hot-reload (Rust only, no JS build)
```

### Production build (Nix)
```bash
nix build                # produces result/ with the bundled app via package.nix
```

### Python environment (notebooks)
```bash
uv sync                  # install/update Python deps from pyproject.toml + uv.lock
uv run marimo edit       # start marimo manually (app does this automatically)
```

No test suite exists. `cargo check` and `cargo clippy` work normally.

## Architecture

Everything lives in `src/lib.rs` (~280 lines). Key pieces:

**Startup sequence** (`run()` → `setup` hook):
1. `spawn_marimo()` — forks `uv run marimo edit --watch --headless --no-token -p 2730` into its own process group (Unix) so SIGKILL can hit the whole tree.
2. `build_marimo_window()` — creates the main WebviewWindow pointed at `dist/index.html` (a static loading spinner). The real navigation to `http://localhost:2730` happens once `wait_for_server()` confirms the port is open.
3. `wait_for_server()` — polls TCP connect every 200 ms for up to 30 s.

**Custom title bar** (`titlebar_script()`):
Injected as an initialization script into the main window only. Native decorations are disabled (`decorations(false)`). The script:
- Injects CSS that pushes `<body>` down 36 px using `position:absolute` + `transform:translateZ(0)`. The transform is critical — it makes `<body>` the containing block for Marimo's internally `position:fixed` elements, so they respect the 36 px offset. `position:absolute` (not `fixed`) is used so that wheel scroll events bubble through body. `overflow` on body is `auto` on the home page (so the page scrolls normally) and `hidden` inside a notebook (Marimo manages its own scrolling, avoiding a duplicate scrollbar).
- Builds a `<div id="__tb__">` attached to `<html>` (not `<body>`) so the bar is outside the transformed body and stays pinned at the true viewport top.
- Intercepts plain left-clicks on `<a>` tags to navigate in-place; modifier/middle clicks fall through to `on_new_window` and open a new `WebviewWindow`.
- Shows a "← Home" button only when not on the home page (`/`).

**Multi-window** (`on_new_window` + `WINDOW_COUNTER`):
Each new window gets label `marimo-N` (counter starts at 2, `main` is 1). New windows do not get the titlebar injection — they use default decorations.

**Shutdown**:
- Closing the last window shows a confirmation dialog ("Stop Marimo?").
- On confirm, `kill_marimo()` sends SIGKILL to the process group, then destroys the window.
- `on window Destroyed` with no remaining windows also kills marimo (handles force-quit).

**Capabilities** (`capabilities/`):
- `default.json` — `core:default` for all windows.
- `marimo-remote.json` — grants `window.minimize/close/toggleMaximize/startDragging` to content from `http://localhost:2730*`, which is how the injected title-bar JS calls Tauri window APIs without a Rust command.

**`dist/index.html`**: Static loading spinner shown during Marimo server startup. Not a build artifact — checked in.

## Nix packaging notes

`package.nix` uses `rustPlatform.buildRustPackage` with `cargo-tauri.hook` to drive `cargo tauri build`. Three WebKit env vars are forced at wrap time to work around NVIDIA/compositing issues:
- `WEBKIT_DISABLE_DMABUF_RENDERER=1`
- `WEBKIT_DISABLE_COMPOSITING_MODE=1`
- `WEBKIT_FORCE_SANDBOX=0`

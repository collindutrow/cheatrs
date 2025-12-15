
# Cheatrs

Cheatrs is a small Rust/Tauri-based desktop cheatsheet viewer with a Leptos/WASM UI. It provides a toggleable window, fuzzy search, and dynamically loaded JSON cheatsheets.

Cheatrs get it's name from a combination of Cheat and the common abbreviation RS for Rust – which cheatrs is written in.

## Features

- Toggleable cheatsheet window (`Super` + `/`).
- Hides on `Esc` or when losing focus.
- System tray with Toggle, Reload, Open Cheatsheets Folder, Quit.
- Loads cheatsheets from JSON files in predefined directories.
- Fuzzy search within filtered sheet(s).
- Light/dark mode via system theme.
- Allows for both per process and general cheatsheets.


## Adding sheets

Place `.json` files in either:

- `<project>/cheatsheets`
- Any user-specific cheatsheet directory listed above

Use tray → Reload to pick up changes.

## Cheatsheet JSON format

### Sheet
```json
{
  "id": "unique-id",
  "name": "Sheet name",
  "description": "Optional",
  "hint": "Optional",
  "processes": [
    "Optional.exe",
  ],
  "sections": [ /* Section */ ]
}
```

### Section

```json
{
  "title": "Section title",
  "items": [ /* Item */ ]
}
```

### Item

```json
{
  "keys": ["Ctrl+C"],
  "desc": "Copy",
  "tags": ["editing"],
  "hint": "Optional"
}
```

## Cheatsheet discovery paths

Cheatrs loads all `*.json` files (non-recursive) from:

1. **Project directory**
    First ancestor containing `cheatsheets/`:

   ```
   <project>/cheatsheets
   ```

2. **Bundled resources directory**
    Included during packaging:

   ```
   <app-resources>/cheatsheets
   ```

3. **Per-user directory**

   - Windows: `%APPDATA%\cheatrs\cheatsheets`
   - macOS: `~/Library/Application Support/cheatrs/cheatsheets`
   - Linux: `~/.local/share/cheatrs/cheatsheets`

Directories are scanned in the above order. Invalid JSON files are skipped.

## Behavior summary

- Window toggles with global shortcut; hides instead of closing.
- Blur (window loses focus) hides window.
- Type-to-search focuses the search field.
- Tray `Reload` reloads the UI.

## Dependencies

### Frontend (`cheatrs-ui`)

- `leptos` (`csr`)
- `serde`, `serde-wasm-bindgen`
- `wasm-bindgen`, `web-sys`, `js-sys`
- `console_error_panic_hook`

### Backend (`cheatrs`)

- `tauri` (tray-icon, image-png)
- `tauri-plugin-global-shortcut`
- `tauri-plugin-opener`
- `serde`, `serde_json`
- `windows` (Windows only)

## Build and run

Regenerating icons

```shell
cargo tauri icon src-tauri/icons/icon.png
```

Development:

```shell
cargo tauri dev
```

Frontend-only preview:

```shell
trunk serve
```

Packaging:

```shell
cargo tauri build
```

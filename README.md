# URDF Visualizer for Zed

**English** | [简体中文](./README.zh.md)

Interactive 3D URDF & Xacro robot visualizer, ROS `package://` asset resolver, Remote SSH preview forwarder, and URDF Language Server for [Zed](https://zed.dev) — inspired by [`MorningFrog/urdf-visualizer`](https://github.com/MorningFrog/urdf-visualizer).

Because Zed extensions run in a headless WASI sandbox without custom WebView/GUI panel APIs, this extension pairs a lightweight Zed WASM extension with an auto-downloaded companion binary (`zed-urdf-server`) that provides both the URDF Language Server and a native floating 3D preview window (or browser view).

---

## Features

- **Language & Editor Support**
  - Recognizes `.urdf`, `.xacro`, and `.urdf.xacro` files using the `tree-sitter-xml` grammar with URDF/Xacro semantic highlighting
  - Real-time LSP diagnostics (XML syntax errors, duplicate links, unresolved `parent`/`child` links, invalid joint limits, missing mesh assets)
  - Go-to-Definition between `<parent>` / `<child>` and `<link>` declarations, hover summaries (mass, joint limits in `rad` & `deg`, resolved mesh paths), and document outline (`Cmd+Shift+O`)
- **Interactive 3D Visualization**
  - **Unsaved Live Hot-Reload**: Listens to LSP `textDocument/didChange` so geometry, origin, material, and Xacro edits update the 3D scene immediately without saving, preserving current joint angles and camera view
  - **Supported Geometries**: `<box>`, `<cylinder>`, `<sphere>`, `<capsule>`, plus `.stl`, `.dae` (Collada), `.obj`, and `.glb` / `.gltf` meshes
  - **Layer Toggles**: `Visual`, `Collision` (translucent overlay), `Inertia` (3×3 symmetric inertia tensor eigendecomposition principal-axis box + center-of-mass sphere), `TF` coordinate frames, and `Axes` (joint motion arrows)
  - **Joint Manipulation**: Sidebar sliders with explicit **lower / `0` / upper limit labels** (click any limit label to snap directly, toggle `rad` / `deg`, zero all, or randomize pose) plus direct **3D pointer dragging** on robot links
  - **Inspection & Measurement**: Link filter/visibility tree with source jump (`Locate`), two-point 3D distance measurement (with $\Delta X, \Delta Y, \Delta Z$), three-point 3D angle measurement, and live Z-Up ROS hover coordinates
  - **Bilingual UI**: Instant `English` / `简体中文` switching directly from the viewer toolbar or via Zed settings
- **Built-in Xacro & ROS `package://` Resolution**
  - Uses system `xacro` when available, and automatically falls back to a built-in pure-Rust Xacro expander (`xacro:macro`, `xacro:property`, `xacro:arg` live UI overrides, `xacro:include`, `xacro:if`/`unless`, `${...}` math expressions, `$(find pkg)`) — works with zero ROS installation
  - Automatically indexes `package.xml` in the workspace, `AMENT_PREFIX_PATH`, and `ROS_PACKAGE_PATH`, with manual override support in `.zed/settings.json`
- **Native Zed Remote SSH Support**
  - Runs a zero-dependency static `musl` binary on remote Linux servers (no GTK/WebKit needed) and binds deterministic port `19876` on `127.0.0.1` for effortless SSH `LocalForward` previewing

---

## Installation (Build & Install from Source)

This extension is installed locally as a Zed **Dev Extension** rather than from the official Zed Extensions registry.

### Prerequisites

- **Rust via [`rustup`](https://rustup.rs/)** with the WASI target installed (required by Zed to compile the Dev Extension):
  ```sh
  rustup target add wasm32-wasip2
  ```
- **Node.js 20+** *(only required if you want to build `zed-urdf-server` and its embedded Three.js WebUI from source)*

### 1. Clone the Repository

```sh
git clone https://github.com/andrewdiyi/zed-urdf-visualizer.git
cd zed-urdf-visualizer
```

### 2. Install as a Dev Extension in Zed

1. Open Zed and open the command palette (`Cmd+Shift+P` / `Ctrl+Shift+P`)
2. Run **`zed: install dev extension`**
3. Select the cloned `zed-urdf-visualizer` directory

> After pulling updates or editing the WASM extension / language queries, open `zed: extensions` and click **`Rebuild`**.

### 3. Companion Server (`zed-urdf-server`)

#### Option A: Compile from Source (Fully Local / Offline)

1. Build the Three.js WebUI and companion Rust server:
   ```sh
   npm --prefix webui install
   npm --prefix webui run build
   cargo build --release --manifest-path server/Cargo.toml
   ```
2. Tell Zed to use your compiled binary via **either** method:
   - **Point directly in Zed `settings.json`** *(easiest)*:
     ```json
     {
       "lsp": {
         "urdf-lsp": {
           "binary": {
             "path": "/absolute/path/to/zed-urdf-visualizer/server/target/release/zed-urdf-server",
             "arguments": ["lsp"]
           }
         }
       }
     }
     ```
   - **Or copy into Zed's extension work directory**:
     - **macOS** *(remove any existing file first on Apple Silicon to keep ad-hoc code signing valid)*:
       ```sh
       ZED_WORK="$HOME/Library/Application Support/Zed/extensions/work/urdf-visualizer/zed-urdf-server-local"
       mkdir -p "$ZED_WORK"
       rm -f "$ZED_WORK/zed-urdf-server"
       cp server/target/release/zed-urdf-server "$ZED_WORK/zed-urdf-server"
       chmod +x "$ZED_WORK/zed-urdf-server"
       ```
     - **Linux**:
       ```sh
       ZED_WORK="$HOME/.local/share/zed/extensions/work/urdf-visualizer/zed-urdf-server-local"
       mkdir -p "$ZED_WORK"
       cp server/target/release/zed-urdf-server "$ZED_WORK/zed-urdf-server"
       chmod +x "$ZED_WORK/zed-urdf-server"
       ```
     - **Windows (PowerShell)**:
       ```powershell
       $ZedWork = "$env:LOCALAPPDATA\Zed\extensions\work\urdf-visualizer\zed-urdf-server-local"
       New-Item -ItemType Directory -Force -Path $ZedWork | Out-Null
       Copy-Item "server\target\release\zed-urdf-server.exe" "$ZedWork\zed-urdf-server.exe"
       ```
3. Open any `.urdf` / `.xacro` file (or run **`editor: restart language server`** in Zed).

#### Option B: Use Prebuilt Binary from GitHub Releases

If you skip Option A and directly open a `.urdf` or `.xacro` file after Step 2, the extension will automatically download the matching prebuilt `zed-urdf-server` binary from [GitHub Releases](https://github.com/andrewdiyi/zed-urdf-visualizer/releases) into Zed's extension work directory.

---

## Opening the 3D Preview

Preview does **not** auto-open when opening a file (`auto_open_viewer: false` by default). Trigger it anytime via:

- **Gutter Play Button (`▶`)**: Click `▶` next to `<robot ...>` and select `◎ URDF: Open 3D Preview` or `↗ URDF: Open 3D Preview in Browser` *(macOS / Linux; Windows users without `sh` can use CodeLens or `Ctrl+.` below)*
- **Inline CodeLens**: Set `"code_lens": "on"` in Zed `settings.json` to display clickable `◎ Open 3D Preview` / `↗ Open in Browser` buttons directly above `<robot ...>`
- **Code Actions (`Cmd+.` / `Ctrl+.`)**: Run `URDF Visualizer: Open 3D Preview`, `Open 3D Preview in Browser`, or `Rescan ROS Packages` from anywhere in the file
- **Zed Tasks (`Cmd+Shift+P` → `task: spawn`)**: Run `◎ URDF: Open 3D Preview`
- **Standalone CLI**:
  ```sh
  zed-urdf-server serve path/to/robot.urdf
  zed-urdf-server serve path/to/robot.xacro --browser --pkg my_pkg=/path/to/my_pkg
  ```

---

## Remote SSH Workflow

When editing robot models on a remote Linux host via Zed Remote SSH, `urdf-lsp` automatically detects the SSH session and binds `127.0.0.1:19876`.

### Option A (Recommended): Add `LocalForward` to `~/.ssh/config`

```sshconfig
Host my-robot-server
    HostName your-server.example.com
    Port 22
    User ubuntu
    LocalForward 19876 127.0.0.1:19876
```

Zed's SSH connection automatically forwards port `19876`. Open `http://127.0.0.1:19876` in your browser, or launch a pinned native floating window locally:

```sh
zed-urdf-server window --url "http://127.0.0.1:19876/?native=1" --pin
```

### Option B: One-Command Tunnel + Native Floating Window

```sh
zed-urdf-server ssh user@your-server.example.com
```

This starts the SSH tunnel in the background, opens the native floating 3D window locally, and tears down the tunnel automatically when the window closes.

> **Restricted network / offline servers**: Configure `"github_proxy": "https://your-mirror"` in `.zed/settings.json`, or place the Linux `musl` binary once at `~/.local/share/zed/remote_extensions/work/urdf-visualizer/zed-urdf-server-v0.1.2/zed-urdf-server` — cached binaries load offline without contacting GitHub.

---

## Configuration (`.zed/settings.json`)

```json
{
  "code_lens": "on",
  "lsp": {
    "urdf-lsp": {
      "settings": {
        "auto_open_viewer": false,
        "prefer_browser": false,
        "always_on_top": true,
        "language": "en",
        "remote_port": 19876,
        "remote_allow_lan": false,
        "github_repo": "andrewdiyi/zed-urdf-visualizer",
        "github_proxy": "",
        "packages": {
          "my_robot_description": "${workspaceFolder}/src/my_robot_description"
        },
        "xacro_args": {
          "with_gripper": "true"
        }
      }
    }
  }
}
```

---

## Release Assets

GitHub Actions (`.github/workflows/release.yml`) builds and publishes five self-contained archives (with the Three.js WebUI embedded via `rust-embed`) for every `v*` tag:

- `zed-urdf-server-aarch64-apple-darwin.tar.gz` (macOS Apple Silicon, native `WKWebView` window)
- `zed-urdf-server-x86_64-apple-darwin.tar.gz` (macOS Intel, native `WKWebView` window)
- `zed-urdf-server-x86_64-unknown-linux-musl.tar.gz` (Linux x86_64 static `musl`, zero dynamic dependencies)
- `zed-urdf-server-aarch64-unknown-linux-musl.tar.gz` (Linux ARM64 static `musl`, zero dynamic dependencies)
- `zed-urdf-server-x86_64-pc-windows-msvc.zip` (Windows x64, native `WebView2` window)

Each archive places the executable (`zed-urdf-server` or `zed-urdf-server.exe`) at the archive root.

---

## Troubleshooting

- **Inspect logs**: Run `zed: open log` in Zed (or launch from terminal with `zed --foreground`) to see `urdf-lsp` startup and download logs.
- **GitHub download timeout / TLS error**: Set `"github_proxy"` in `.zed/settings.json` or manually place the release binary in the extension work directory listed above.
- **Missing `package://` meshes**: Check the **ROS Packages & Xacro** accordion in the viewer sidebar to see discovered packages, or add explicit mappings under `lsp.urdf-lsp.settings.packages`.
- **macOS Homebrew Rust dev-build error (`can't find crate for core`)**: Zed's dev extension builder requires `rustup`'s `cargo` / `rustc` (not Homebrew's standalone `rust` formula) to be first on `PATH` with `wasm32-wasip2` installed.

---

## Project Structure

```text
.
├── .github/workflows/release.yml   # 5-platform release build & publish workflow
├── Cargo.toml                      # Zed WASM extension crate (cdylib)
├── extension.toml                  # Zed extension manifest (grammars & LSP)
├── src/lib.rs                      # Extension lifecycle & platform binary auto-downloader
├── languages/
│   ├── urdf/                       # URDF config, highlights, runnables, tasks
│   └── xacro/                      # Xacro config, highlights, runnables, tasks
├── server/                         # Standalone Rust companion binary (zed-urdf-server)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                 # CLI modes: lsp, window, serve, open-preview, ssh
│       ├── lsp_server.rs           # LSP server (diagnostics, hover, goto-def, CodeLens, CodeActions)
│       ├── http_server.rs          # Embedded WebUI, WebSocket live sync, /pkg & /fs asset serving
│       ├── native_window.rs        # Native floating WKWebView / WebView2 window (macOS & Windows)
│       ├── xacro.rs                # External xacro CLI + built-in pure-Rust Xacro expander
│       ├── packages.rs             # ROS 1 / ROS 2 package.xml & workspace path resolver
│       ├── analyzer.rs             # URDF/Xacro semantic analyzer & symbol index
│       └── state.rs                # Shared state, settings & SSH session detection
├── webui/                          # Vite + TypeScript + Three.js 3D frontend (embedded into server)
│   └── src/
│       ├── viewer.ts               # Z-Up Three.js scene, mesh/capsule loaders, 3D joint dragging
│       ├── inertia.ts              # 3×3 symmetric inertia tensor Jacobi eigensolver & box visualizer
│       ├── measurement.ts          # 3D distance & angle measurement tools
│       ├── i18n.ts                 # English / Simplified Chinese localization
│       └── main.ts                 # WebSocket sync, inspector panels & native IPC
└── examples/                       # Ready-to-test 6-DOF Xacro arm + STL mesh demo package
```

---

## Acknowledgements

This project builds on and draws inspiration from:

- [`MorningFrog/urdf-visualizer`](https://github.com/MorningFrog/urdf-visualizer) & [`MorningFrog/zed-urdf`](https://github.com/MorningFrog/zed-urdf)
- [`gkjohnson/urdf-loaders`](https://github.com/gkjohnson/urdf-loaders) & [Three.js](https://threejs.org/)
- [`tree-sitter-xml`](https://github.com/tree-sitter-grammars/tree-sitter-xml) & [`sweetppro/zed-xml`](https://github.com/sweetppro/zed-xml)
- [`tauri-apps/wry`](https://github.com/tauri-apps/wry) & [`tauri-apps/tao`](https://github.com/tauri-apps/tao)

---

## License

This project is licensed under the [MIT License](./LICENSE).

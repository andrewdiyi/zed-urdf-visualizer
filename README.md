# Zed URDF Visualizer

**English** | [简体中文](./README.zh.md)

An interactive 3D URDF & Xacro robot visualizer, ROS `package://` asset resolver, Remote SSH preview forwarder, and URDF Language Server for the [Zed](https://zed.dev) editor — inspired by [`MorningFrog/urdf-visualizer`](https://github.com/MorningFrog/urdf-visualizer).

---

## Features

1. **Zero-Setup Zed Extension Experience**
   - Install the extension directly in Zed — no manual Rust, Node.js, or ROS installation required.
   - On first use, the extension automatically detects your platform (macOS Apple Silicon / Intel, Linux `x86_64` / `aarch64` static `musl`, Windows `x64`), downloads the self-contained `zed-urdf-server` binary from GitHub Releases into Zed's private extension sandbox, and manages it automatically.
2. **Unsaved Live Hot-Reload**
   - Powered by a custom `urdf-lsp` Language Server listening to `textDocument/didChange`: edits to link dimensions, joint origins, materials, or Xacro properties update the 3D scene in real time **without saving**, preserving joint poses and camera angle.
3. **Click-to-Preview Triggers (No Unwanted Auto-Popups)**
   - Preview does not auto-open by default (`auto_open_viewer: false`).
   - Click the **`▶`** runnable button in the editor gutter on `<robot ...>`, click the **`◎ Open 3D Preview`** / **`↗ Open in Browser`** CodeLens (with `"code_lens": "on"`), or press `Cmd+.` (`Ctrl+.`) anywhere in a `.urdf` / `.xacro` file.
4. **Full Robotics Inspection Toolkit**
   - **Mesh Formats**: `.stl`, `.dae` (Collada), `.obj`, `.glb` / `.gltf`, and standard URDF primitives (`box`, `cylinder`, `sphere`).
   - **Layer Toggles**: `Visual`, `Collision` (translucent amber overlay), `Inertia` (3×3 symmetric inertia tensor eigendecomposition principal-axis box + center-of-mass marker), `TF` frames, and `Axes` (joint motion arrows).
   - **Interactive Joint Controls**: Sidebar sliders with **explicit lower / `0` / upper limit labels** (click any limit label to snap directly, toggle `rad` / `deg`, randomize pose, or zero all joints) plus **direct 3D pointer dragging** on robot links.
   - **Link Tree & Source Navigation**: Search/filter links, toggle individual link visibility, and jump straight to `<link>` / `<joint>` definitions in Zed.
   - **3D Measurement Tools**: Two-point 3D distance measurement (with $\Delta X, \Delta Y, \Delta Z$) and three-point 3D angle measurement (degrees & radians), plus live Z-Up ROS coordinates on hover.
   - **Built-in Pure-Rust Xacro Engine & `package://` Resolver**: Uses system `xacro` when available and automatically falls back to a built-in pure-Rust Xacro expander (`xacro:macro`, `xacro:property`, `xacro:arg` live overrides, `xacro:include`, `xacro:if`/`unless`, `${...}` math expressions, `$(find pkg)`). Automatically scans `package.xml`, `AMENT_PREFIX_PATH`, and `ROS_PACKAGE_PATH`.
5. **Native Zed Remote SSH Forwarding**
   - When opening `.urdf` / `.xacro` files over **Zed Remote SSH** on headless Linux robotics/RL servers:
     - Runs a zero-dependency static `musl` binary on the remote host (no GTK/WebKit required);
     - Automatically detects SSH sessions and binds deterministic port `19876` on `127.0.0.1` so SSH `LocalForward` works without colliding with local Zed instances.

---

## Opening the 3D Preview

- **Editor Gutter Play Button**: Click **`▶`** next to `<robot ...>` and select `◎ URDF: Open 3D Preview` or `↗ URDF: Open 3D Preview in Browser`.
- **Inline CodeLens**: Enable `"code_lens": "on"` in Zed `settings.json` to click `◎ Open 3D Preview` directly above `<robot ...>`.
- **Code Actions (`Cmd+.` / `Ctrl+.`)**: Trigger native floating window, browser preview, or ROS package rescan from anywhere in the file.
- **Zed Tasks (`Cmd+Shift+P` → `task: spawn`)**: Run `◎ URDF: Open 3D Preview` (works with zero `$PATH` setup).

---

## Remote SSH Workflow

When editing robot models on a remote Linux server via Zed Remote SSH, `urdf-lsp` listens on `127.0.0.1:19876` on the remote host.

### Option A (Recommended): Add `LocalForward` to `~/.ssh/config`

```sshconfig
Host my-robot-server
    HostName your-server.example.com
    Port 22
    User ubuntu
    LocalForward 19876 127.0.0.1:19876
```

Zed's SSH connection automatically forwards port `19876`. Open `http://127.0.0.1:19876` in your local browser (or launch a pinned native floating window locally with `zed-urdf-server window --url "http://127.0.0.1:19876/?native=1" --pin`).

### Option B: One-Command Tunnel + Native Floating Window

```bash
zed-urdf-server ssh user@your-server.example.com
```

This creates the SSH tunnel in the background and opens a native always-on-top 3D preview window locally, automatically closing the tunnel when the window exits.

> **Restricted network / offline servers**: Configure `"github_proxy": "https://your-mirror"` in `.zed/settings.json`, or place the Linux `musl` binary once in `~/.local/share/zed/remote_extensions/work/urdf-visualizer/zed-urdf-server-v0.1.1/zed-urdf-server` — cached binaries load offline without contacting GitHub.

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

## License

[MIT](./LICENSE)

# URDF Visualizer for Zed

[English](./README.md) | **简体中文**

为 [Zed](https://zed.dev) 编辑器打造的交互式 3D URDF / Xacro 机器人可视化插件、ROS `package://` 资源解析器、Remote SSH 远端转发预览工具与 URDF 语言服务器（LSP）——灵感来自 [`MorningFrog/urdf-visualizer`](https://github.com/MorningFrog/urdf-visualizer)。

由于 Zed 插件运行在无界面的 WASI 沙盒中且未开放自定义 WebView/GUI 面板接口，本扩展采用「轻量 Zed WASM 插件 + 自动下载的伴生单文件服务（`zed-urdf-server`）」架构，同时提供编辑器 LSP 能力与原生悬浮 3D 预览窗（及浏览器双模式）。

---

## 核心特性

- **语言与编辑器增强**
  - 基于 `tree-sitter-xml` 识别 `.urdf`、`.xacro`、`.urdf.xacro` 并提供 URDF/Xacro 语义高亮
  - 实时 LSP 诊断（XML 语法错误、重复 Link、未定义的 `parent`/`child` 引用、关节限位异常、缺失网格路径告警）
  - 支持 `<parent>` / `<child>` 到 `<link>` 的 Go-to-Definition 跳转、悬停查看连杆质量/角度限位（`rad` 与 `deg`）/本地网格路径以及大纲视图（`Cmd+Shift+O`）
- **交互式 3D 可视化**
  - **未保存毫秒级热更新**：监听 LSP `textDocument/didChange`，编辑几何体、原点、材质或 Xacro 参数时无需保存即可实时刷新 3D 场景，并自动保留当前关节姿态与相机视角
  - **支持几何体与网格**：`<box>`、`<cylinder>`、`<sphere>`、`<capsule>`（胶囊体），以及 `.stl`、`.dae` (Collada)、`.obj`、`.glb` / `.gltf` 网格
  - **图层一键切换**：`Visual`（外观）、`Collision`（半透明碰撞包络）、`Inertia`（3×3 对称惯性张量特征分解等效主轴盒 + 质心球）、`TF` 坐标系与 `Axes` 关节运动轴箭头
  - **关节交互控制**：侧边栏滑块明示 **下限 / `0` / 上限**（点击数值快速吸附跳转，支持 `rad` / `deg` 切换、全部归零、随机姿态），并支持**在 3D 视图中直接鼠标拖拽连杆运动**
  - **检查与空间测量**：连杆搜索过滤与显隐开关、点击反向定位 Zed 源码行、两点三维测距（含 $\Delta X, \Delta Y, \Delta Z$）、三点空间测角及悬停实时 ROS（Z-Up）坐标读数
  - **中英双语界面**：支持在 3D 窗口顶栏一键实时切换 `English` / `简体中文`，或在 Zed 配置中指定默认语言
- **内置纯 Rust Xacro 引擎与 `package://` 自动寻址**
  - 优先调用系统 `xacro`，在无 ROS 环境（如 macOS 原生）自动启用内置纯 Rust Xacro 展开器（支持 `xacro:macro`、`xacro:property`、`xacro:arg` 在线覆盖、`xacro:include`、`xacro:if`/`unless`、`${...}` 数学表达式与 `$(find pkg)`）
  - 自动扫描工作区 `package.xml`、`AMENT_PREFIX_PATH` 与 `ROS_PACKAGE_PATH`，也支持在 `.zed/settings.json` 手动配置映射
- **原生支持 Zed Remote SSH 转发**
  - 远端 Linux 自动运行**零动态库依赖的静态 `musl` 二进制**（无需 GTK/WebKit），自动检测 SSH 会话并固定监听 `127.0.0.1:19876`，配合 `LocalForward` 即可在本机实时预览远端模型

---

## 安装方式

普通用户**无需安装 Rust、Node.js、Python 或 ROS**：

1. 在 Zed 扩展市场安装 **URDF Visualizer**
2. 打开任意 `.urdf` 或 `.xacro` 文件
3. 首次启动时，插件会检查 Zed 私有工作目录中是否已有缓存的 `zed-urdf-server`；若不存在，则自动从 GitHub Releases 下载对应平台的单文件压缩包并解压启动

---

## 如何唤起 3D 预览

打开文件默认**不会**自动弹窗（`auto_open_viewer: false`），随时可通过以下方式打开：

- **行号左侧 `▶` 槽口按钮**：点击 `<robot ...>` 行左侧的 `▶`，选择 `◎ URDF: Open 3D Preview` 或 `↗ URDF: Open 3D Preview in Browser` *（macOS / Linux 开箱即用；无 `sh` 的 Windows 可使用下方的 CodeLens 或 `Ctrl+.`）*
- **行内 CodeLens 按钮**：在 Zed `settings.json` 中启用 `"code_lens": "on"` 后，可直接点击 `<robot ...>` 正上方的 `◎ Open 3D Preview` / `↗ Open in Browser`
- **快捷菜单（`Cmd+.` / `Ctrl+.`）**：在文件任意位置按 `Cmd+.` 选择打开 3D 预览、浏览器预览或重新扫描 ROS Package
- **Zed Tasks（`Cmd+Shift+P` → `task: spawn`）**：运行 `◎ URDF: Open 3D Preview`
- **独立命令行模式**：
  ```sh
  zed-urdf-server serve path/to/robot.urdf
  zed-urdf-server serve path/to/robot.xacro --browser --pkg my_pkg=/path/to/my_pkg
  ```

---

## Zed Remote SSH 远端机器人工程预览

通过 Zed Remote SSH 在远端 Linux 主机打开 `.urdf` / `.xacro` 时，`urdf-lsp` 会自动检测 SSH 会话并固定监听 `127.0.0.1:19876`。

### 方式 A（推荐）：在客户端 `~/.ssh/config` 加一行 `LocalForward`

```sshconfig
Host my-robot-server
    HostName your-server.example.com
    Port 22
    User ubuntu
    LocalForward 19876 127.0.0.1:19876
```

Zed 自身的 SSH 连接会自动打通 `19876` 隧道。直接在本机浏览器打开 `http://127.0.0.1:19876`，或运行下方命令打开本机原生置顶悬浮窗：

```sh
zed-urdf-server window --url "http://127.0.0.1:19876/?native=1" --pin
```

### 方式 B：一条命令建立隧道并弹出本机原生悬浮窗

```sh
zed-urdf-server ssh user@your-server.example.com
```

会自动在后台拉起隧道并弹出原生置顶 3D 窗口，关闭窗口时自动断开隧道。

> **受限网络 / 离线服务器**：可在 `.zed/settings.json` 配置 `"github_proxy": "https://your-mirror"`，或手动将 Linux `musl` 二进制放到远端 `~/.local/share/zed/remote_extensions/work/urdf-visualizer/zed-urdf-server-v0.1.2/zed-urdf-server`，插件会优先离线加载本地缓存。

---

## 配置项参考（`.zed/settings.json`）

```json
{
  "code_lens": "on",
  "lsp": {
    "urdf-lsp": {
      "settings": {
        "auto_open_viewer": false,
        "prefer_browser": false,
        "always_on_top": true,
        "language": "zh",
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

## 发布产物（Release Assets）

每推送一个 `v*` 标签，GitHub Actions（`.github/workflows/release.yml`）会自动构建并发布 5 个单文件预编译包（已通过 `rust-embed` 内嵌 Three.js 前端）：

- `zed-urdf-server-aarch64-apple-darwin.tar.gz`（macOS Apple Silicon，系统原生 `WKWebView` 悬浮窗）
- `zed-urdf-server-x86_64-apple-darwin.tar.gz`（macOS Intel，系统原生 `WKWebView` 悬浮窗）
- `zed-urdf-server-x86_64-unknown-linux-musl.tar.gz`（Linux x86_64 纯静态 `musl`，零动态库依赖）
- `zed-urdf-server-aarch64-unknown-linux-musl.tar.gz`（Linux ARM64 纯静态 `musl`，零动态库依赖）
- `zed-urdf-server-x86_64-pc-windows-msvc.zip`（Windows x64，系统原生 `WebView2` 悬浮窗）

每个压缩包根目录直接包含可执行文件（`zed-urdf-server` 或 Windows 下的 `zed-urdf-server.exe`）。

---

## 本地开发与调试

### 环境要求

- 通过 [`rustup`](https://rustup.rs/) 安装 Rust 并添加 `wasm32-wasip2` 目标（`rustup target add wasm32-wasip2`）
- Node.js 20+（用于构建 Three.js 前端至 `server/dist/`）

### 作为开发扩展安装到 Zed

1. 打开 Zed，按 `Cmd+Shift+P` / `Ctrl+Shift+P` 打开命令面板
2. 运行 `zed: install dev extension`
3. 选择本仓库根目录

修改 WASM 扩展或语法查询后，在 `zed: extensions` 页面点击 **`Rebuild`** 即可。

### 无需发布 GitHub Release 的本地调试流程

1. 本地构建前端与伴生服务：
   ```sh
   npm --prefix webui install
   npm --prefix webui run build
   cargo build --release --manifest-path server/Cargo.toml
   ```
2. 将二进制复制到 Zed 扩展沙盒工作目录：
   - **macOS** *（Apple Silicon 需先 `rm -f` 再 `cp`，避免内核旧签名缓存拦截）*：
     ```sh
     ZED_WORK="$HOME/Library/Application Support/Zed/extensions/work/urdf-visualizer/zed-urdf-server-v0.1.2"
     mkdir -p "$ZED_WORK"
     rm -f "$ZED_WORK/zed-urdf-server"
     cp server/target/release/zed-urdf-server "$ZED_WORK/zed-urdf-server"
     chmod +x "$ZED_WORK/zed-urdf-server"
     ```
   - **Linux**：
     ```sh
     ZED_WORK="$HOME/.local/share/zed/extensions/work/urdf-visualizer/zed-urdf-server-v0.1.2"
     mkdir -p "$ZED_WORK"
     cp server/target/release/zed-urdf-server "$ZED_WORK/zed-urdf-server"
     chmod +x "$ZED_WORK/zed-urdf-server"
     ```
   - **Windows (PowerShell)**：
     ```powershell
     $ZedWork = "$env:LOCALAPPDATA\Zed\extensions\work\urdf-visualizer\zed-urdf-server-v0.1.2"
     New-Item -ItemType Directory -Force -Path $ZedWork | Out-Null
     Copy-Item "server\target\release\zed-urdf-server.exe" "$ZedWork\zed-urdf-server.exe"
     ```
3. 在 Zed 中运行 **`editor: restart language server`** 即可加载新版服务。

---

## 常见问题排查（Troubleshooting）

- **查看详细日志**：在 Zed 中执行 `zed: open log`，或在终端运行 `zed --foreground` 查看 `urdf-lsp` 启动与下载日志。
- **GitHub 下载超时 / TLS 错误**：在 `.zed/settings.json` 配置 `"github_proxy"`，或手动将对应平台压缩包解压到上方扩展工作目录中。
- **`package://` 网格加载失败**：展开 3D 视图侧边栏的 **ROS Packages & Xacro** 面板检查当前解析到的包路径，或在 `lsp.urdf-lsp.settings.packages` 中手动指定路径映射。
- **macOS Homebrew Rust 开发编译报错 `can't find crate for core`**：Zed 的 Dev Extension 编译器需要 `PATH` 中优先命中 `rustup` 的 `cargo`/`rustc`（而非 Homebrew 独立版 `rust`）并已安装 `wasm32-wasip2`。

---

## 项目结构

```text
.
├── .github/workflows/release.yml   # 五端自动编译与 GitHub Release 发布工作流
├── Cargo.toml                      # Zed WASM 扩展包配置（cdylib）
├── extension.toml                  # Zed 扩展清单（语法与 LSP 注册）
├── src/lib.rs                      # 插件生命周期与跨平台二进制自动下载缓存
├── languages/
│   ├── urdf/                       # URDF 语言配置、语法高亮、槽口运行器与任务
│   └── xacro/                      # Xacro 语言配置、语法高亮、槽口运行器与任务
├── server/                         # 独立 Rust 伴生服务（zed-urdf-server）
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                 # CLI 子命令：lsp、window、serve、open-preview、ssh
│       ├── lsp_server.rs           # LSP 服务端（诊断、悬停、跳转、CodeLens、CodeActions）
│       ├── http_server.rs          # 内嵌 WebUI、WebSocket 实时同步、/pkg 与 /fs 资源服务
│       ├── native_window.rs        # 原生悬浮 WKWebView / WebView2 窗口（macOS & Windows）
│       ├── xacro.rs                # 外部 xacro CLI + 内置纯 Rust Xacro 展开器
│       ├── packages.rs             # ROS 1 / ROS 2 package.xml 与工作区路径解析
│       ├── analyzer.rs             # URDF/Xacro 语义分析与符号索引
│       └── state.rs                # 共享状态、用户配置与 SSH 会话检测
├── webui/                          # Vite + TypeScript + Three.js 3D 前端（编译后内嵌进 server）
│   └── src/
│       ├── viewer.ts               # Z-Up Three.js 场景、网格/胶囊体加载、3D 拖拽关节
│       ├── inertia.ts              # 3×3 对称惯性张量 Jacobi 特征分解与等效主轴盒
│       ├── measurement.ts          # 三维两点测距与三点测角工具
│       ├── i18n.ts                 # English / 简体中文双语切换
│       └── main.ts                 # WebSocket 同步、检查器面板与原生窗口 IPC
└── examples/                       # 开箱即用的 6 自由度机械臂 Xacro + STL 示例包
```

---

## 致谢

本项目基于并参考了以下优秀开源项目：

- [`MorningFrog/urdf-visualizer`](https://github.com/MorningFrog/urdf-visualizer) 与 [`MorningFrog/zed-urdf`](https://github.com/MorningFrog/zed-urdf)
- [`gkjohnson/urdf-loaders`](https://github.com/gkjohnson/urdf-loaders) 与 [Three.js](https://threejs.org/)
- [`tree-sitter-xml`](https://github.com/tree-sitter-grammars/tree-sitter-xml) 与 [`sweetppro/zed-xml`](https://github.com/sweetppro/zed-xml)
- [`tauri-apps/wry`](https://github.com/tauri-apps/wry) 与 [`tauri-apps/tao`](https://github.com/tauri-apps/tao)

---

## 开源协议

本项目基于 [MIT License](./LICENSE) 开源。

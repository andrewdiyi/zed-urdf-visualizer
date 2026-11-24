# Zed URDF Visualizer

[English](./README.md) | **简体中文**

在 **Zed** 编辑器中实现与 [`MorningFrog/urdf-visualizer`](https://github.com/MorningFrog/urdf-visualizer) 对齐的交互式 3D URDF / Xacro 机器人可视化插件，原生支持 **零环境依赖自动下载** 与 **Zed Remote SSH 远端机器人工程实时转发预览**。

---

## 核心特性

1. **纯 Zed 插件安装体验（零手动依赖）**
   - 用户只需安装 Zed 并安装本扩展：首次打开 `.urdf` / `.xacro` 文件时，插件会自动识别当前系统架构（macOS Apple Silicon / Intel、Linux `x86_64` / `aarch64` 静态 `musl`、Windows `x64`），从 GitHub Releases 自动下载单文件服务到 Zed 专属沙盒目录并运行，无需手动安装 Node、Rust 或 ROS。
2. **Zed 未保存毫秒级实时热更新**
   - 通过 `urdf-lsp` 监听 `textDocument/didChange`，修改关节原点、连杆尺寸或 Xacro 参数时**无需保存文件**，3D 视图实时同步刷新，并保持当前关节角度与相机视角。
3. **按需点击打开（默认不自动弹窗）**
   - 默认关闭自动弹窗（`auto_open_viewer: false`）。
   - 在 `<robot ...>` 行号左侧直接点击 **`▶`** 槽口按钮，或在开启 `"code_lens": "on"` 后点击 `<robot>` 上方的 **`◎ 打开 3D 预览`** / **`↗ 浏览器 3D 预览`**，也可随时按 `Cmd+.`（`Ctrl+.`）一键唤起。
4. **原生支持 Zed Remote SSH 转发**
   - 在通过 Zed SSH 连接远端 Linux 机器人/强化学习服务器（如 ROS 2 Humble、IsaacLab、MJLab 等无桌面服务器）时：
     - 自动拉取**零动态库依赖的 Linux `musl` 静态二进制**到远端 Zed 沙盒；
     - 自动检测 SSH 会话并固定监听 `127.0.0.1:19876`（避免与本机端口冲突），配合 `~/.ssh/config` 一行 `LocalForward 19876 127.0.0.1:19876` 或一键 `zed-urdf-server ssh` 命令即可在本机悬浮窗/浏览器实时预览远端模型与网格资源。
5. **完整机器人调试工具箱**
   - 支持 `.stl`、`.dae` (Collada)、`.obj`、`.glb` / `.gltf` 网格与标准原语几何体。
   - 一键切换 `Visual`、`Collision`（半透明碰撞包络）、`Inertia`（对称惯性张量特征分解主轴包络 + 质心球）、`TF` 坐标轴与 `Axes` 关节轴箭头。
   - 支持侧边栏滑块（**明示各关节下限 / `0` / 上限且支持点击快速跳转**、`rad` / `deg` 切换、随机姿态、全部归零）及 **3D 视图鼠标直接拖拽关节**。
   - 内置 **两点三维测距**（含 $\Delta X, \Delta Y, \Delta Z$）与 **三点空间夹角测量**。
   - 内置纯 Rust Xacro 展开器（无 ROS 环境亦可展开 `xacro:macro`、`xacro:property`、`xacro:arg`、`${...}` 数学表达式与 `$(find pkg)`）+ 自动扫描 `package.xml` 解析 `package://`。

---

## 如何唤起 3D 预览

- **行号左侧 `▶` 槽口按钮**：点击 `<robot ...>` 行左侧的 `▶`，选择 `◎ URDF: 打开原生 3D 悬浮窗预览` 或 `↗ URDF: 在浏览器中打开 3D 预览`。
- **行内 CodeLens 按钮**：在 Zed 设置中添加 `"code_lens": "on"` 后，可直接点击 `<robot ...>` 正上方的 `◎ 打开 3D 预览`。
- **快捷菜单 (`Cmd + .`)**：在编辑器任意位置按 `Cmd + .` 选择打开悬浮窗、浏览器或重新扫描 ROS Package。
- **Zed Tasks (`Cmd + Shift + P` → `task: spawn`)**：选择 `◎ URDF: 打开原生 3D 悬浮窗预览`（内部通过本地 HTTP 接口触发，无需配置全局 `$PATH`）。

---

## Zed Remote SSH 远端机器人工程预览

当你在本机用 Zed Remote SSH 连接远端 Linux 主机打开 `.urdf` / `.xacro` 时，远端 `urdf-lsp` 会自动解析远端的 ROS 包与网格并监听固定端口 **`19876`**。任选以下一种方式即可在本机查看 3D 画面：

### 方式 A（最推荐，一劳永逸）：在客户端 `~/.ssh/config` 加一行转发
```sshconfig
Host my-robot-server
    HostName your-server.example.com
    Port 22
    User ubuntu
    LocalForward 19876 127.0.0.1:19876
```
这样 **Zed 自身的 SSH 连接就会自动帮你打通 `19876` 隧道**，无需额外开终端！直接在本机浏览器打开 `http://127.0.0.1:19876`（或本机运行 `zed-urdf-server window --url "http://127.0.0.1:19876/?native=1" --pin` 打开置顶悬浮窗）即可实时联动。

### 方式 B：一键建立 SSH 隧道并弹出原生置顶悬浮窗
```bash
zed-urdf-server ssh user@your-server.example.com
```
会自动在后台拉起隧道并弹出原生置顶 3D 窗口，关闭窗口时自动断开隧道。

> **若远端服务器无法直连 GitHub 下载 Release**：
> 可在远端 `.zed/settings.json` 配置代理前缀 `"github_proxy": "https://your-mirror"`，或者将 Linux 静态二进制放到远端 `~/.local/share/zed/remote_extensions/work/urdf-visualizer/zed-urdf-server-v0.1.2/zed-urdf-server`（或 `~/.local/bin/zed-urdf-server`），插件会优先离线秒加载本地缓存，不卡网络请求。

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

## 开源协议

[MIT](./LICENSE)

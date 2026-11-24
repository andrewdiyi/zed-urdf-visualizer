#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_BIN_DIR="${HOME}/.local/bin"

echo "==> [1/3] 构建 Three.js 3D WebUI 前端资源..."
npm --prefix "${ROOT_DIR}/webui" install
npm --prefix "${ROOT_DIR}/webui" run build

echo "==> [2/3] 编译 Rust 伴生服务 (zed-urdf-server --release)..."
cargo build --release --manifest-path "${ROOT_DIR}/server/Cargo.toml"

mkdir -p "${INSTALL_BIN_DIR}"
rm -f "${INSTALL_BIN_DIR}/zed-urdf-server"
cp "${ROOT_DIR}/server/target/release/zed-urdf-server" "${INSTALL_BIN_DIR}/zed-urdf-server"
chmod +x "${INSTALL_BIN_DIR}/zed-urdf-server"
echo "    已安装至: ${INSTALL_BIN_DIR}/zed-urdf-server"

echo "==> [3/3] 检查 Zed WASM 扩展编译环境 (rustup + wasm32-wasip2)..."
if [ -d "/opt/homebrew/opt/rustup/bin" ]; then
  ln -sf /opt/homebrew/opt/rustup/bin/cargo \
         /opt/homebrew/opt/rustup/bin/rustc \
         /opt/homebrew/opt/rustup/bin/rustdoc \
         /opt/homebrew/opt/rustup/bin/rustfmt \
         /opt/homebrew/opt/rustup/bin/rustup \
         "${INSTALL_BIN_DIR}/"
  export PATH="${INSTALL_BIN_DIR}:${PATH}"
fi

if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-wasip2
  cargo build --release --target wasm32-wasip2 --manifest-path "${ROOT_DIR}/Cargo.toml"
  echo "    WASM 扩展预检编译成功！"
else
  echo "⚠️  提示: 当前系统使用的是 Homebrew Rust 而非 rustup。"
  echo "   Zed 的 'Install Dev Extension' 需要通过 rustup 安装 wasm32-wasip2 目标："
  echo "   brew install rustup && \"\$(brew --prefix rustup)/bin/rustup-init\" -y && rustup target add wasm32-wasip2"
fi

echo ""
echo "🎉 构建完成！接下来在 Zed 中："
echo "   1. 按 Cmd+Shift+P 输入 'zed: extensions'"
echo "   2. 点击右上角 'Install Dev Extension' 并选择目录: ${ROOT_DIR}"
echo "   3. 打开 ${ROOT_DIR}/examples/sample_6dof_arm.urdf.xacro 即可自动弹出 3D 悬浮窗实时预览！"

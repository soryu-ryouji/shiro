#!/usr/bin/env bash
# 发包：构建 shiro 桌面应用的分发包并归置到仓库根目录的 out/。
# macOS 产物为 shiro-mac-<arch>.zip（.app 目录压缩包），Linux 产物为 shiro-linux-x64.AppImage。
#
# 用法: ./tools/build.sh
# 前置: 最新 Node.js 与 Rust 工具链（https://rustup.rs/）
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$REPO_ROOT/shiro-app"
OUT_DIR="$REPO_ROOT/out"

for tool in node npm cargo; do
  command -v "$tool" >/dev/null 2>&1 || { echo "未找到 $tool，请先安装最新的 Node.js 与 Rust 工具链（https://rustup.rs/）"; exit 1; }
done

cd "$APP_DIR"
[ -d node_modules ] || npm install
npm run pack

mkdir -p "$OUT_DIR"
case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64) ARCH=arm64 ;;
      x86_64) ARCH=x64 ;;
      *) echo "不支持的架构: $(uname -m)"; exit 1 ;;
    esac
    APP="$(ls -d dist/mac*/shiro.app 2>/dev/null | head -n1)"
    [ -n "$APP" ] || { echo "打包产物不存在: dist/mac*/shiro.app（electron-builder 未产出）"; exit 1; }
    ditto -c -k --sequesterRsrc --keepParent "$APP" "$OUT_DIR/shiro-mac-$ARCH.zip"
    echo "应用分发包: $OUT_DIR/shiro-mac-$ARCH.zip"
    ;;
  Linux)
    APPIMAGE="$(ls dist/*.AppImage 2>/dev/null | head -n1)"
    [ -n "$APPIMAGE" ] || { echo "打包产物不存在: dist/*.AppImage（electron-builder 未产出）"; exit 1; }
    cp "$APPIMAGE" "$OUT_DIR/"
    chmod +x "$OUT_DIR/$(basename "$APPIMAGE")"
    echo "应用分发包: $OUT_DIR/$(basename "$APPIMAGE")"
    ;;
  *)
    echo "不支持的平台: $(uname -s)"; exit 1
    ;;
esac

echo "完成：全部产物已归置到 $OUT_DIR。"

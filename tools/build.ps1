# 发包：构建 shiro 桌面应用的分发包并归置到仓库根目录的 out/（Windows 为免安装 shiro-windows-x64.zip）。
#
# 用法: ./tools/build.ps1
# 前置: 最新 Node.js 与 Rust 工具链（https://rustup.rs/）
# 压缩级别（默认 5）: $env:ELECTRON_BUILDER_COMPRESSION_LEVEL=9; ./tools/build.ps1   # 9=最小体积，3=最快
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$AppDir = Join-Path $RepoRoot 'shiro-app'
$OutDir = Join-Path $RepoRoot 'out'

foreach ($tool in @('node', 'npm', 'cargo')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "未找到 $tool，请先安装最新的 Node.js 与 Rust 工具链（https://rustup.rs/）"
    }
}

Push-Location $AppDir
try {
    if (-not (Test-Path 'node_modules')) {
        npm install
    }
    npm run pack
} finally {
    Pop-Location
}

$package = Join-Path $AppDir 'release\shiro-windows-x64.zip'
if (-not (Test-Path $package)) {
    throw "打包产物不存在: $package（electron-builder 未产出 shiro-windows-x64.zip）"
}
New-Item -ItemType Directory -Force $OutDir | Out-Null
Copy-Item $package (Join-Path $OutDir 'shiro-windows-x64.zip') -Force
Write-Host "应用分发包: $OutDir\shiro-windows-x64.zip"

Write-Host ""
Write-Host "完成：全部产物已归置到 $OutDir。" -ForegroundColor Green

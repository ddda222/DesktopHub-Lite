param([string]$OutputRoot = 'D:\codex\DesktopHub-work\packages')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$env:CARGO_HOME = 'D:\codex\DesktopHub-work\rust\cargo'
$env:RUSTUP_HOME = 'D:\codex\DesktopHub-work\rust\rustup'
$env:CARGO_TARGET_DIR = 'D:\codex\DesktopHub-work\rust\target'
$env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
Push-Location $projectRoot
try {
    & pnpm exec vite build
    if ($LASTEXITCODE -ne 0) { throw '前端打包失败' }
    & "$env:CARGO_HOME\bin\cargo.exe" build --release --manifest-path (Join-Path $projectRoot 'src-tauri\Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw '桌面程序编译失败' }
    $packageName = 'DesktopHub-Lite-0.1.0-win-x64-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
    $packageDirectory = Join-Path $OutputRoot $packageName
    New-Item -ItemType Directory -Path $packageDirectory -Force | Out-Null
    Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR\release\desktophub-lite.exe" -Destination (Join-Path $packageDirectory 'DesktopHub-Lite.exe')
    New-Item -ItemType File -Path (Join-Path $packageDirectory 'portable.flag') | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'PORTABLE.md') -Destination (Join-Path $packageDirectory '使用说明.md')
    $archive = Join-Path $OutputRoot "$packageName.zip"
    Compress-Archive -LiteralPath $packageDirectory -DestinationPath $archive -CompressionLevel Optimal
    Write-Output "便携包：$archive"
    Write-Output "启动程序：$(Join-Path $packageDirectory 'DesktopHub-Lite.exe')"
} finally { Pop-Location }

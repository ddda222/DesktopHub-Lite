# 独立验证当前源码中的规则与 Core Audio 只读枚举，不启动 Tauri、不设置音量。
param([switch]$ReadAudio)
$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$target = 'D:\codex\DesktopHub-work\native-tools-check'
New-Item -ItemType Directory -Path (Join-Path $target 'src') -Force | Out-Null
$audio = Get-Content -LiteralPath (Join-Path $project 'src-tauri\src\audio.rs') -Raw
$compare = Get-Content -LiteralPath (Join-Path $project 'src-tauri\src\compare.rs') -Raw
function Section([string]$Source, [string]$Start, [string]$End) {
    $from = $Source.IndexOf($Start, [StringComparison]::Ordinal)
    if ($from -lt 0) { throw "缺少源码边界：$Start" }
    $to = if ($End) { $Source.IndexOf($End, $from, [StringComparison]::Ordinal) } else { $Source.Length }
    if ($to -lt $from) { throw "缺少源码结束边界：$End" }
    $Source.Substring($from, $to - $from)
}
$source = 'use serde::{Deserialize, Serialize};' + "`n"
$source += Section $audio '#[derive(Clone, Serialize)]' 'async fn run'
$source += Section $audio '#[cfg(windows)]' '#[cfg(not(windows))]'
$source += Section $audio '#[cfg(test)]' ''
$source += "`nmod file_preview { pub const IMAGE_LIMIT: u64 = 8 * 1024 * 1024; }`nmod compare { use serde::{Deserialize, Serialize};`n"
$source += Section $compare '#[derive(Clone, Deserialize, Serialize)]' '#[derive(Default)]'
$source += Section $compare 'fn validate(' '#[tauri::command]'
$source += Section $compare '#[cfg(test)]' ''
$source += "`n}`n"
Set-Content -LiteralPath (Join-Path $target 'src\lib.rs') -Value $source -Encoding utf8
@'
[package]
name = "desktophub-native-tools-check"
version = "0.0.0"
edition = "2021"
[dependencies]
serde = { version = "1", features = ["derive"] }
sha2 = "0.10"
windows = { version = "0.62", features = ["Win32_Foundation", "Win32_Media_Audio", "Win32_System_Com", "Win32_System_Com_StructuredStorage", "Win32_System_Variant", "Win32_System_Threading", "Win32_UI_Shell_PropertiesSystem"] }
'@ | Set-Content -LiteralPath (Join-Path $target 'Cargo.toml') -Encoding utf8
$env:CARGO_HOME = 'D:\codex\DesktopHub-work\rust\cargo'
$env:RUSTUP_HOME = 'D:\codex\DesktopHub-work\rust\rustup'
$env:CARGO_TARGET_DIR = 'D:\codex\DesktopHub-work\rust\target'
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
$nativeLinker = 'D:\codex\DesktopHub-work\build-tools\llvm-mingw-20261104-msvcrt-x86_64\bin'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $nativeLinker 'x86_64-w64-mingw32-gcc.exe'
$env:RUSTFLAGS = '-C link-self-contained=yes'
$env:PATH = "$env:CARGO_HOME\bin;$nativeLinker;$env:PATH"
$manifest = Join-Path $target 'Cargo.toml'
& "$env:CARGO_HOME\bin\cargo.exe" test --offline --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($ReadAudio) {
    & "$env:CARGO_HOME\bin\cargo.exe" test --offline --manifest-path $manifest read_only_windows_sessions -- --ignored --nocapture
    exit $LASTEXITCODE
}

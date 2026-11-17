# 隔离验证持久化和配置格式；只使用 D 盘临时样例，不启动桌面程序。
$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$target = 'D:\codex\DesktopHub-work\core-check'
New-Item -ItemType Directory -Path (Join-Path $target 'src') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $project 'src-tauri\src\storage.rs') -Destination (Join-Path $target 'src\storage.rs')
$source = '#![allow(dead_code)]' + "`n" + '#[macro_export] macro_rules! log_warn { ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }}; }' + "`npub mod storage;`n"
if (Test-Path (Join-Path $project 'src-tauri\src\config_format.rs')) {
    function Section([string]$Text, [string]$Start, [string]$End) {
        $from = $Text.IndexOf($Start, [StringComparison]::Ordinal)
        $to = if ($End) { $Text.IndexOf($End, $from, [StringComparison]::Ordinal) } else { $Text.Length }
        if ($from -lt 0 -or $to -lt $from) { throw '源码边界不存在' }
        $Text.Substring($from, $to - $from)
    }
    $apps = Get-Content (Join-Path $project 'src-tauri\src\applications.rs') -Raw
    $appearance = Get-Content (Join-Path $project 'src-tauri\src\desktop_features.rs') -Raw
    $source += "mod applications { use serde::{Deserialize, Serialize};`n" + (Section $apps 'pub const CONFIG_FILE' 'fn cat(') + "`n}`n"
    $source += "mod desktop_features { use serde::{Deserialize, Serialize};`n" + (Section $appearance '#[derive(Clone, Debug, Deserialize, Serialize)]' '#[tauri::command]') + "`n}`n"
    Copy-Item (Join-Path $project 'src-tauri\src\state.rs') (Join-Path $target 'src\state.rs')
    Copy-Item (Join-Path $project 'src-tauri\src\config_format.rs') (Join-Path $target 'src\config_format.rs')
    $source += "mod state; mod config_format;`n"
}
$source | Set-Content (Join-Path $target 'src\lib.rs') -Encoding utf8
@'
[package]
name = "desktophub-core-check"
version = "0.0.0"
edition = "2021"
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
'@ | Set-Content (Join-Path $target 'Cargo.toml') -Encoding utf8
$env:CARGO_HOME = 'D:\codex\DesktopHub-work\rust\cargo'
$env:RUSTUP_HOME = 'D:\codex\DesktopHub-work\rust\rustup'
$env:CARGO_TARGET_DIR = 'D:\codex\DesktopHub-work\rust\target'
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
$env:HUB_LITE_TEST_ROOT = Join-Path $target 'fixtures'
$nativeLinker = 'D:\codex\DesktopHub-work\build-tools\llvm-mingw-20261104-msvcrt-x86_64\bin'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $nativeLinker 'x86_64-w64-mingw32-gcc.exe'
$env:RUSTFLAGS = '-C link-self-contained=yes'
$env:PATH = "$env:CARGO_HOME\bin;$nativeLinker;$env:PATH"
& "$env:CARGO_HOME\bin\cargo.exe" test --offline --manifest-path (Join-Path $target 'Cargo.toml')
exit $LASTEXITCODE

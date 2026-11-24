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
    $source += "mod desktop_features { use serde::{Deserialize, Serialize};`n" + (Section $appearance '#[derive(Clone, Debug, Deserialize, Serialize)]' '#[tauri::command]') + "`n" + (Section $appearance '// === STARTUP_CORE_START ===' '// === STARTUP_CORE_END ===') + "`n}`n"
    Copy-Item (Join-Path $project 'src-tauri\src\state.rs') (Join-Path $target 'src\state.rs')
    Copy-Item (Join-Path $project 'src-tauri\src\config_format.rs') (Join-Path $target 'src\config_format.rs')
    $telemetry = Get-Content (Join-Path $project 'src-tauri\src\telemetry.rs') -Raw
    $telemetryCheck = (Section $telemetry 'use serde' '/// 启动后台每秒按需遥测线程') + "`n" + (Section $telemetry '#[cfg(test)]' '')
    $telemetryCheck | Set-Content (Join-Path $target 'src\telemetry.rs') -Encoding utf8
    $dataLoc = Get-Content (Join-Path $project 'src-tauri\src\data_location.rs') -Raw
    $dataLocCore = (Section $dataLoc '//! 数据目录定位' '// === TAURI_COMMANDS ===') + "`n" + (Section $dataLoc '// === TESTS ===' '')
    $dataLocCore | Set-Content (Join-Path $target 'src\data_location.rs') -Encoding utf8
    $ocr = Get-Content (Join-Path $project 'src-tauri\src\ocr.rs') -Raw
    $ocrCore = (Section $ocr '//! 屏幕截图捕获' '// === TAURI_COMMANDS ===') + "`n" + (Section $ocr '// === TESTS ===' '')
    $ocrCore | Set-Content (Join-Path $target 'src\ocr.rs') -Encoding utf8
    $source += "mod state; mod config_format; mod telemetry; mod data_location; mod ocr;`n"
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
$env:HUB_LITE_TEST_ROOT = Join-Path $target 'fixtures'
$gnuBin = Join-Path $env:RUSTUP_HOME 'toolchains\stable-x86_64-pc-windows-gnu\bin'
$msvcBin = Join-Path $env:RUSTUP_HOME 'toolchains\stable-x86_64-pc-windows-msvc\bin'
$nativeLinker = 'D:\codex\DesktopHub-work\build-tools\llvm-mingw-20261104-msvcrt-x86_64\bin'
if ((Test-Path (Join-Path $gnuBin 'rustc.exe')) -and (Test-Path (Join-Path $nativeLinker 'x86_64-w64-mingw32-gcc.exe'))) {
    $env:RUSTC = Join-Path $gnuBin 'rustc.exe'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $nativeLinker 'x86_64-w64-mingw32-gcc.exe'
    $env:RUSTFLAGS = '-C link-self-contained=yes'
    $env:PATH = "$gnuBin;$nativeLinker;$env:PATH"
    $cargoExe = Join-Path $gnuBin 'cargo.exe'
} elseif (Test-Path (Join-Path $msvcBin 'rustc.exe')) {
    $env:RUSTC = Join-Path $msvcBin 'rustc.exe'
    $env:PATH = "$msvcBin;$env:PATH"
    $cargoExe = Join-Path $msvcBin 'cargo.exe'
} else {
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
    $cargoExe = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
}
& $cargoExe test --offline --manifest-path (Join-Path $target 'Cargo.toml')
exit $LASTEXITCODE

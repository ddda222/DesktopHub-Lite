param(
    [ValidateSet('Edge','WebView2')][string]$Mode = 'Edge',
    [string]$WorkRoot = 'D:\codex\DesktopHub-work',
    [string]$ScreenshotDir = ''
)
$ErrorActionPreference = 'Stop'
$target = Join-Path $WorkRoot 'motion-qa-20261118'
New-Item -ItemType Directory -Path $target -Force | Out-Null
$env:HUB_QA_OUTPUT = $target
$env:HUB_QA_MEDIA = $ScreenshotDir
$env:HUB_QA_WEBVIEW_HOST = ''
if (-not $env:NODE_PATH) {
    $env:NODE_PATH = Join-Path $env:USERPROFILE '.cache\codex-runtimes\codex-primary-runtime\dependencies\node\node_modules'
}
if ($Mode -eq 'WebView2') {
    $source = (Join-Path $PSScriptRoot 'verify-motion-runtime.rs').Replace('\','/')
    $manifest = "[package]`nname='hub-motion-wv2-probe'`nversion='0.1.0'`nedition='2021'`n[[bin]]`nname='hub-motion-wv2-probe'`npath='$source'`n[dependencies]`ntao='=0.37.2'`nwry='=0.57.2'"
    $manifestPath = Join-Path $target 'Cargo.toml'
    Set-Content -LiteralPath $manifestPath -Encoding utf8 -Value $manifest
    $env:CARGO_HOME = Join-Path $WorkRoot 'rust\cargo'
    $env:RUSTUP_HOME = Join-Path $WorkRoot 'rust\rustup'
    $env:CARGO_TARGET_DIR = Join-Path $WorkRoot 'rust\target'
    $env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
    $env:RUSTFLAGS = '-C link-self-contained=yes'
    $llvm = Join-Path $WorkRoot 'build-tools\llvm-mingw-20261104-msvcrt-x86_64\bin'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $llvm 'x86_64-w64-mingw32-gcc.exe'
    $env:PATH = "$llvm;$env:CARGO_HOME\bin;$env:PATH"
    & "$env:CARGO_HOME\bin\cargo.exe" build --offline --manifest-path $manifestPath
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
    $env:HUB_QA_WEBVIEW_HOST = Join-Path $env:CARGO_TARGET_DIR 'debug\hub-motion-wv2-probe.exe'
} elseif (-not $env:HUB_QA_BROWSER) {
    $env:HUB_QA_BROWSER = Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'
}
& node (Join-Path $PSScriptRoot 'verify-motion-runtime.mjs')
exit $LASTEXITCODE

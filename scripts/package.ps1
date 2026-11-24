#requires -Version 7.0
param(
    [string]$OutputRoot = 'D:\codex\DesktopHub-work\packages',
    [string]$ReleaseNotesFile = '',
    [switch]$IsolatedPortable
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if ($ReleaseNotesFile) { $ReleaseNotesFile = (Resolve-Path -LiteralPath $ReleaseNotesFile).Path }
$version = (Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json).version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw '无效的便携版版本号' }
$tauriVersion = (Get-Content -LiteralPath (Join-Path $projectRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$cargoManifest = Get-Content -LiteralPath (Join-Path $projectRoot 'src-tauri\Cargo.toml') -Raw
if ($version -ne $tauriVersion -or $cargoManifest -notmatch ('(?m)^version = "' + [regex]::Escape($version) + '"\r?$')) { throw '前端与桌面版本号不一致' }
$env:CARGO_HOME = 'D:\codex\DesktopHub-work\rust\cargo'
$env:RUSTUP_HOME = 'D:\codex\DesktopHub-work\rust\rustup'
$env:CARGO_TARGET_DIR = 'D:\codex\DesktopHub-work\rust\target'
$env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
# 优先复用完整的本地工具链，避免 rustup 代理意外触发联网同步。
$llvmBin = 'D:\codex\DesktopHub-work\build-tools\llvm-mingw-20261104-msvcrt-x86_64\bin'
$gnuToolchain = Join-Path $env:RUSTUP_HOME 'toolchains\stable-x86_64-pc-windows-gnu'
$cargoExe = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
if ((Test-Path -LiteralPath (Join-Path $gnuToolchain 'bin\rustc.exe')) -and (Test-Path -LiteralPath "$llvmBin\x86_64-w64-mingw32-gcc.exe")) {
    $env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "$llvmBin\x86_64-w64-mingw32-gcc.exe"
    $env:RUSTFLAGS = '-C link-self-contained=yes'
    $env:PATH = "$llvmBin;$env:PATH"
} else {
    # GNU 工具链不完整时直连本地 MSVC 工具链，显式固定 RUSTC 与 cargo.exe
    $msvcBin = Join-Path $env:RUSTUP_HOME 'toolchains\stable-x86_64-pc-windows-msvc\bin'
    if ((Test-Path -LiteralPath (Join-Path $msvcBin 'cargo.exe')) -and (Test-Path -LiteralPath (Join-Path $msvcBin 'rustc.exe'))) {
        $env:RUSTC = Join-Path $msvcBin 'rustc.exe'
        $cargoExe = Join-Path $msvcBin 'cargo.exe'
    }
}
Push-Location $projectRoot
try {
    & pnpm run build
    if ($LASTEXITCODE -ne 0) { throw '前端打包失败' }
    $nativeDirectory = Join-Path $projectRoot 'src-tauri'
    $distDirectory = if ($env:HUB_LITE_OUT_DIR) { (Resolve-Path -LiteralPath $env:HUB_LITE_OUT_DIR).Path } else { 'D:\codex\DesktopHub-work\desktophub-lite-dist' }
    if (!(Test-Path -LiteralPath (Join-Path $distDirectory 'index.html'))) { throw '前端入口文件不存在' }
    # Tauri 先按 URL 解析 frontendDist；D:/... 会变成外部地址，必须传相对路径。
    $relativeDist = [System.IO.Path]::GetRelativePath($nativeDirectory, $distDirectory).Replace('\', '/')
    if ([System.IO.Path]::IsPathRooted($relativeDist)) { throw '前端产物和项目必须位于同一磁盘' }
    $env:TAURI_CONFIG = @{ build = @{ frontendDist = $relativeDist } } | ConvertTo-Json -Compress
    & $cargoExe build --release --offline --locked --manifest-path (Join-Path $nativeDirectory 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw '桌面程序编译失败' }
    $packageName = "DesktopHub-Lite-$version-win-x64-" + (Get-Date -Format 'yyyyMMdd-HHmmss')
    $packageDirectory = Join-Path $OutputRoot $packageName
    New-Item -ItemType Directory -Path $packageDirectory -Force | Out-Null
    Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR\release\desktophub-lite.exe" -Destination (Join-Path $packageDirectory 'DesktopHub-Lite.exe')
    Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR\release\WebView2Loader.dll" -Destination $packageDirectory
    if ($IsolatedPortable) {
        New-Item -ItemType File -Path (Join-Path $packageDirectory 'portable-isolated.flag') | Out-Null
    }
    Copy-Item -LiteralPath (Join-Path $projectRoot 'PORTABLE.md') -Destination (Join-Path $packageDirectory '使用说明.md')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'integrations') -Destination $packageDirectory -Recurse
    $releaseNotes = if ($ReleaseNotesFile) { $ReleaseNotesFile } else { Join-Path $projectRoot "docs\releases\lite-v$version.md" }
    if (Test-Path -LiteralPath $releaseNotes) { Copy-Item -LiteralPath $releaseNotes -Destination (Join-Path $packageDirectory '更新说明.md') }
    $sourceCommit = & git -C $projectRoot rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw '无法读取源码提交号' }
    $sourceStatus = @(& git -C $projectRoot status --porcelain --untracked-files=normal)
    if ($LASTEXITCODE -ne 0) { throw '无法读取源码修改状态' }
    [ordered]@{ Product = 'DesktopHub Lite'; Version = $version; SourceCommit = $sourceCommit; SourceDirty = $sourceStatus.Count -gt 0; Platform = 'windows-x64'; BuiltAtUtc = [DateTime]::UtcNow.ToString('o') } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $packageDirectory 'release-metadata.json') -Encoding utf8
    $archive = Join-Path $OutputRoot "$packageName.zip"
    Compress-Archive -LiteralPath $packageDirectory -DestinationPath $archive -CompressionLevel Optimal
    Write-Output "便携包：$archive"
    Write-Output "启动程序：$(Join-Path $packageDirectory 'DesktopHub-Lite.exe')"
} finally { Pop-Location }

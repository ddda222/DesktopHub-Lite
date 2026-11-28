# Read-only Windows application discovery. Embedded in the executable.
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)

$codex = [bool](Get-Command codex.exe,codex.cmd -CommandType Application | Select-Object -First 1)
if (!$codex) {
    $codex = [bool](Get-Process -Name Codex -ErrorAction SilentlyContinue | Select-Object -First 1)
}
if (!$codex) {
    $codex = [bool](Get-StartApps | Where-Object { $_.AppID -like 'OpenAI.Codex_*!*' } | Select-Object -First 1)
}

function Find-Dsh {
    $name = 'DeepSeek Harness.exe'
    $candidates = @()
    $candidates += @(Get-Process -Name 'DeepSeek Harness' | Select-Object -ExpandProperty Path)
    $cli = Get-Command dsh.cmd -CommandType Application | Select-Object -First 1
    if ($cli) { $candidates += [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $cli.Path) "../../../../$name")) }
    $shell = New-Object -ComObject WScript.Shell
    foreach ($folder in @('Programs','CommonPrograms','DesktopDirectory','CommonDesktopDirectory')) {
        $directory = [Environment]::GetFolderPath($folder)
        if (!$directory) { continue }
        foreach ($rel in @('DeepSeek Harness.lnk','DeepSeek Harness/DeepSeek Harness.lnk')) {
            $link = Join-Path $directory $rel
            if ([IO.File]::Exists($link) -and $shell) { $candidates += $shell.CreateShortcut($link).TargetPath }
        }
    }
    foreach ($root in @('HKCU:\Software','HKLM:\Software','HKLM:\Software\WOW6432Node')) {
        $catalog = Join-Path $root 'Microsoft\Windows\CurrentVersion\Uninstall'
        foreach ($app in (Get-ItemProperty "$catalog\*" | Where-Object { $_.DisplayName -eq 'DeepSeek Harness' })) {
            if ($app.InstallLocation) { $candidates += Join-Path $app.InstallLocation $name }
            if ($app.DisplayIcon) { $candidates += ($app.DisplayIcon -replace ',\s*-?\d+$','').Trim('"') }
        }
    }
    foreach ($path in $candidates) {
        if ($path -and [IO.Path]::GetFileName($path) -ieq $name -and [IO.File]::Exists($path)) {
            return [IO.Path]::GetFullPath($path)
        }
    }
}

function Find-Orca {
    $candidates = @()
    $cli = Get-Command orca.exe -CommandType Application | Select-Object -First 1
    if ($cli) { $candidates += $cli.Path }
    foreach ($proc in @(Get-Process -Name 'Orca' -ErrorAction SilentlyContinue)) {
        if ($proc.Path) {
            $candidates += Join-Path (Split-Path -Parent $proc.Path) 'resources\bin\orca.exe'
        }
    }
    if ($env:LOCALAPPDATA) {
        $candidates += Join-Path $env:LOCALAPPDATA 'Programs\orca\resources\bin\orca.exe'
    }
    if ($env:ProgramFiles) {
        $candidates += Join-Path $env:ProgramFiles 'Orca\resources\bin\orca.exe'
    }
    foreach ($path in $candidates) {
        if ($path -and [IO.Path]::GetFileName($path) -ieq 'orca.exe' -and [IO.File]::Exists($path)) {
            return [IO.Path]::GetFullPath($path)
        }
    }
}

$dsh = Find-Dsh
$orca = Find-Orca
[Console]::WriteLine((@{ codex=$codex; dsh=$dsh; orca=$orca } | ConvertTo-Json -Compress))

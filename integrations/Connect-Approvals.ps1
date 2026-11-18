#requires -Version 7.0
param(
    [Parameter(Mandatory)][ValidateSet('Codex','DSH')][string]$应用,
    [string]$程序路径 = (Join-Path (Split-Path -Parent $PSScriptRoot) 'DesktopHub-Lite.exe')
)
$ErrorActionPreference = 'Stop'
$exe = (Get-Item -LiteralPath $程序路径).FullName
if ([IO.Path]::GetExtension($exe) -ine '.exe') { throw '请选择包含审批功能的 DesktopHub Lite 程序。' }

$root = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'DesktopHub-Lite/Approvals'
[IO.Directory]::CreateDirectory($root) | Out-Null
function Write-AtomicText([string]$path, [string]$value) {
    $temporary = "$path.$([guid]::NewGuid()).tmp"
    try {
        [IO.File]::WriteAllText($temporary, $value, [Text.UTF8Encoding]::new($false))
        [IO.File]::Move($temporary, $path, $true)
    } finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
}

if ($应用 -eq 'DSH') {
    $plugin = Join-Path $root 'dsh-desktophub-approvals'
    [IO.Directory]::CreateDirectory($plugin) | Out-Null
    foreach ($name in @('questions.js','index.js','package.json','cordis.patch.yml')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot "dsh-approvals/$name") -Destination $plugin -Force
    }
    Write-AtomicText (Join-Path $plugin 'executable.json') (@{ path = $exe; registration = [guid]::NewGuid().ToString() } | ConvertTo-Json)
    Write-Output "请在 DSH 的插件管理中选择从本地目录安装，选择 $plugin 。若界面提示重启，请按提示操作。"
    return
}

$configDir = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }
[IO.Directory]::CreateDirectory($configDir) | Out-Null
$path = Join-Path $configDir 'hooks.json'
$original = if ([IO.File]::Exists($path)) { [IO.File]::ReadAllText($path) }
$config = if ($null -eq $original) { @{} } else { ConvertFrom-Json -AsHashtable -NoEnumerate -InputObject $original -Depth 100 }
if ($config -isnot [Collections.IDictionary]) { throw '现有 hooks.json 格式无法识别，已停止接入。' }
if (!$config.Contains('hooks')) { $config.hooks = @{} }
if ($config.hooks -isnot [Collections.IDictionary]) { throw '现有 hooks 节点格式无法识别，已停止接入。' }
if ($config.hooks.Contains('PermissionRequest') -and $config.hooks.PermissionRequest -isnot [array]) {
    throw '现有审批钩子格式无法识别，已停止接入。'
}

# The hook shell parses only fixed text. The chosen executable path never enters a shell command.
$winShell = (Join-Path ([Environment]::SystemDirectory) 'WindowsPowerShell/v1.0/powershell.exe').Replace('\','/')
if ($winShell -match '\s') { $winShell = '"' + $winShell + '"' }
$command = $winShell + ' -NoLogo -NoProfile -NonInteractive -Command "& ([IO.Path]::Combine([Environment]::GetFolderPath(''LocalApplicationData''),''DesktopHub-Lite'',''Approvals'',''codex.ps1''))"'
$present = @($config.hooks.PermissionRequest | ForEach-Object { $_.hooks } | Where-Object { $_.command -ceq $command }).Count -gt 0
if (!$present) {
    $entry = @{ matcher = '*'; hooks = @(@{ type = 'command'; command = $command; timeout = 270; statusMessage = '等待灵动岛审批' }) }
    $config.hooks.PermissionRequest = @(@($config.hooks.PermissionRequest) | Where-Object { $null -ne $_ }) + @($entry)
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'codex.ps1') -Destination $root -Force
Write-AtomicText (Join-Path $root 'executable.txt') $exe
if (!$present) {
    if ([IO.File]::Exists($path)) {
        if ([IO.File]::ReadAllText($path) -cne $original) { throw '审批配置已被其他程序修改，请重试。' }
        [IO.File]::Copy($path, "$path.desktophub-$([guid]::NewGuid()).bak")
    } elseif ($null -ne $original) { throw '审批配置已移动，请重试。' }
    Write-AtomicText $path ($config | ConvertTo-Json -Depth 100)
}
Write-Output '已登记 Codex 审批钩子。请在 Codex 设置的钩子页面检查并信任“等待灵动岛审批”。'

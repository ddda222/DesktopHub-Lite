# Windows PowerShell 5.1 compatible. Preserve the hook's UTF-8 input byte for byte.
$ErrorActionPreference = 'Stop'
$child = $null
try {
    $exe = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'executable.txt'))
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $exe
    $info.Arguments = "--codex-approval $PID"
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $child = [Diagnostics.Process]::Start($info)
    [Console]::OpenStandardInput().CopyTo($child.StandardInput.BaseStream)
    $child.StandardInput.BaseStream.Close()
    $reply = $child.StandardOutput.ReadToEnd()
    $child.WaitForExit()
    if ($child.ExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($reply)) { throw 'Unavailable' }
    [Console]::Out.WriteLine($reply.Trim())
} catch {
    [Console]::Out.WriteLine('{}')
} finally {
    if ($null -ne $child) { $child.Dispose() }
}

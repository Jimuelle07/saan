#Requires -Version 5.1
<#
.SYNOPSIS
    Installs saan: builds the CLI and desktop app, copies them under -Prefix and
    puts `saan` on the user PATH.

.DESCRIPTION
    Usage:
        powershell -ExecutionPolicy Bypass -File scripts/install.ps1
        powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -SkipBuild
        powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Uninstall
        powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Uninstall -Purge

    The desktop launcher finds the model because the model lives at
    <Prefix>\models\embeddinggemma-300m, a `models/` ancestor of <Prefix>\bin.

.PARAMETER Prefix
    Install root. Default: %LOCALAPPDATA%\saan.

.PARAMETER SkipBuild
    Reuse existing target\release binaries instead of building.

.PARAMETER Uninstall
    Remove saan from the user PATH and delete <Prefix>\bin. Models and app data
    are kept unless -Purge is also given.

.PARAMETER Purge
    With -Uninstall, delete the whole <Prefix> (bin, models, app data).
#>
[CmdletBinding()]
param(
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA 'saan'),
    [switch]$SkipBuild,
    [switch]$Uninstall,
    [switch]$Purge
)

$ErrorActionPreference = 'Stop'

$Prefix = [System.IO.Path]::GetFullPath($Prefix)
$RepoRoot = Split-Path -Parent $PSScriptRoot
$BinDir = Join-Path $Prefix 'bin'
$ModelDir = Join-Path $Prefix 'models\embeddinggemma-300m'
$ExeSuffix = '.exe'

function Get-UserPath {
    # Read HKCU\Environment\Path straight from the registry, unexpanded, so
    # entries written as %USERPROFILE%\bin survive a round-trip. Both
    # [Environment]::GetEnvironmentVariable('Path','User') and a plain
    # GetValue() expand REG_EXPAND_SZ values, which would persist the expanded
    # text if it were ever written back.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    if ($null -eq $key) {
        return $null
    }
    try {
        return [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    }
    finally {
        $key.Close()
    }
}

function Set-UserPath([string]$Value) {
    # Write back through the registry, preserving the value's original kind.
    # [Environment]::SetEnvironmentVariable always stores REG_SZ, which would
    # permanently flatten %VAR% entries; keep REG_EXPAND_SZ (the default when
    # Path does not exist yet) so expansion still happens at use time.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    if ($null -eq $key) {
        throw 'Cannot open HKCU:\Environment for writing.'
    }
    try {
        $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
        if ($key.GetValueNames() -contains 'Path') {
            $kind = $key.GetValueKind('Path')
        }
        $key.SetValue('Path', $Value, $kind)
    }
    finally {
        $key.Close()
    }
    # Writing the registry alone does not notify running processes. Broadcast
    # WM_SETTINGCHANGE via Environment.SetEnvironmentVariable: a null delete of
    # a nonexistent name is a no-op write that still makes .NET broadcast, and
    # it avoids an Add-Type/P-Invoke block (no compile cost, no SendMessage
    # signature to keep in sync).
    [Environment]::SetEnvironmentVariable('SAAN_PATH_REFRESH', $null, 'User')
}

function Add-ToUserPath([string]$Dir) {
    $current = Get-UserPath
    $parts = @()
    if (-not [string]::IsNullOrEmpty($current)) {
        $parts = @($current -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    }
    $target = $Dir.TrimEnd('\')
    foreach ($part in $parts) {
        if ($part.TrimEnd('\') -eq $target) {
            return
        }
    }
    Set-UserPath ((@($parts) + $Dir) -join ';')
}

function Remove-FromUserPath([string]$Dir) {
    $current = Get-UserPath
    if ([string]::IsNullOrEmpty($current)) {
        return
    }
    $target = $Dir.TrimEnd('\')
    $parts = @($current -split ';' |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and $_.TrimEnd('\') -ne $target })
    Set-UserPath ($parts -join ';')
}

function Invoke-Native([string]$File, [string[]]$Arguments) {
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$File $($Arguments -join ' ') exited with code $LASTEXITCODE"
    }
}

# A running launcher locks saan-app.exe, so stop it (and wait for exit) before
# replacing or deleting the binaries.
function Stop-Launcher {
    $running = @(Get-Process -Name 'saan-app' -ErrorAction SilentlyContinue)
    if ($running.Count -gt 0) {
        $running | Stop-Process -Force
        $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
    }
}

if ($Uninstall) {
    Stop-Launcher
    Remove-FromUserPath $BinDir
    if (Test-Path $BinDir) {
        Remove-Item -Path $BinDir -Recurse -Force
    }
    if ($Purge -and (Test-Path $Prefix)) {
        Remove-Item -Path $Prefix -Recurse -Force
    }
    Write-Host "saan uninstalled. Open a new terminal for the PATH change to apply."
    exit 0
}

if (-not $SkipBuild) {
    Push-Location $RepoRoot
    try {
        if (-not (Test-Path (Join-Path $RepoRoot 'node_modules'))) {
            Invoke-Native 'npm' @('install')
        }
        # `tauri build` embeds the frontend (custom-protocol feature); a plain
        # `cargo build -p saan-app` would point the window at the dev server URL.
        Invoke-Native 'npm' @('run', 'tauri', 'build')
        Invoke-Native 'cargo' @('build', '--release', '-p', 'saan-cli')
    }
    finally {
        Pop-Location
    }
}

Stop-Launcher
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
foreach ($name in @('saan', 'saan-app')) {
    $source = Join-Path $RepoRoot ("target\release\{0}{1}" -f $name, $ExeSuffix)
    if (-not (Test-Path $source)) {
        throw "$source not found; build first (or drop -SkipBuild)"
    }
    Copy-Item -Path $source -Destination (Join-Path $BinDir ("{0}{1}" -f $name, $ExeSuffix)) -Force
}

if (-not (Test-Path (Join-Path $ModelDir 'tokenizer.json'))) {
    $repoModel = Join-Path $RepoRoot 'models\embeddinggemma-300m'
    if (Test-Path (Join-Path $repoModel 'tokenizer.json')) {
        New-Item -ItemType Directory -Force -Path $ModelDir | Out-Null
        Copy-Item -Path (Join-Path $repoModel '*') -Destination $ModelDir -Recurse -Force
    }
    else {
        New-Item -ItemType Directory -Force -Path $ModelDir | Out-Null
        $env:SAAN_MODEL_DIR = $ModelDir
        Invoke-Native (Join-Path $BinDir 'saan.exe') @('fetch-model')
    }
}

Add-ToUserPath $BinDir

Write-Host ''
Write-Host "saan installed in $BinDir"
Write-Host 'Open a NEW terminal and type: saan'
Write-Host "(Add -SkipBuild to reuse existing binaries; -Uninstall to remove.)"

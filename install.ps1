# Installs or updates Agent Roster on Windows. In PowerShell:
#   irm https://raw.githubusercontent.com/itsmahadi007/agent-roster/main/install.ps1 | iex
# It downloads the latest release, installs the app and the `roster` command, and opens the app.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'itsmahadi007/agent-roster'
$isArm = ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') -or ($env:PROCESSOR_ARCHITEW6432 -eq 'ARM64')
$arch = if ($isArm) { 'arm64' } else { 'x64' }
$url = "https://github.com/$repo/releases/latest/download/Agent-Roster-windows-$arch-setup.exe"
$setup = Join-Path $env:TEMP 'Agent-Roster-setup.exe'

Write-Host "Downloading Agent Roster for Windows ($arch)..."
Invoke-WebRequest -Uri $url -OutFile $setup -UseBasicParsing

Get-Process agent-roster-app -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host 'Installing...'
Start-Process -FilePath $setup -ArgumentList '/S' -Wait
Remove-Item $setup -Force -ErrorAction SilentlyContinue

$dir = Join-Path $env:LOCALAPPDATA 'Agent Roster'
$app = Join-Path $dir 'agent-roster-app.exe'
if (-not (Test-Path $app)) { throw "Agent Roster didn't install into $dir." }

# Put the folder with roster.exe on your PATH so `roster` works in new terminals.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $userPath) { $userPath = '' }
if (($userPath -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', (($userPath.TrimEnd(';') + ';' + $dir).TrimStart(';')), 'User')
}

Start-Process -FilePath $app
Write-Host 'Agent Roster is installed and open. It checks for updates by itself.'
Write-Host 'Open a new terminal to use the roster command.'

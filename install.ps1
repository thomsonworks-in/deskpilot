# DeskPilot Windows 1-Click Installer
# Usage: irm https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'

$Repo = 'thomsonworks-in/deskpilot'
$BinaryName = 'deskpilot.exe'
$InstallDir = Join-Path $env:LOCALAPPDATA 'DeskPilot\bin'

Write-Host '=========================================' -ForegroundColor Cyan
Write-Host '       DeskPilot Windows Installer        ' -ForegroundColor Cyan
Write-Host '=========================================' -ForegroundColor Cyan

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
$Target = switch ($Arch) {
    'X64'   { 'x86_64-pc-windows-msvc' }
    'Arm64' { 'aarch64-pc-windows-msvc' }
    Default { 'x86_64-pc-windows-msvc' }
}

Write-Host ('Detected Architecture: ' + $Target) -ForegroundColor Gray

$ApiUrl = 'https://api.github.com/repos/' + $Repo + '/releases/latest'
Write-Host 'Fetching latest release from GitHub...' -ForegroundColor Gray

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $Release = Invoke-RestMethod -Uri $ApiUrl -Headers @{ 'User-Agent' = 'DeskPilot-Installer' }
    $Tag = $Release.tag_name
    Write-Host ('Latest Release: ' + $Tag) -ForegroundColor Green
} catch {
    $Tag = 'v0.1.0'
}

$ZipName = 'deskpilot-' + $Target + '.zip'
$DownloadUrl = 'https://github.com/' + $Repo + '/releases/download/' + $Tag + '/' + $ZipName

if (!(Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$TempZip = Join-Path $env:TEMP 'deskpilot-setup.zip'
Write-Host ('Downloading DeskPilot (' + $ZipName + ')...') -ForegroundColor Cyan
Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempZip

Write-Host 'Extracting...' -ForegroundColor Gray
Expand-Archive -Path $TempZip -DestinationPath $InstallDir -Force
Remove-Item -Force $TempZip

$UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($UserPath -notlike ('*' + $InstallDir + '*')) {
    Write-Host 'Adding DeskPilot to user PATH...' -ForegroundColor Gray
    [Environment]::SetEnvironmentVariable('Path', ($UserPath + ';' + $InstallDir), 'User')
    $env:Path = $env:Path + ';' + $InstallDir
}

try {
    $WshShell = New-Object -ComObject WScript.Shell
    $Programs = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs')
    $Shortcut = $WshShell.CreateShortcut((Join-Path $Programs 'DeskPilot.lnk'))
    $Shortcut.TargetPath = Join-Path $InstallDir $BinaryName
    $Shortcut.WorkingDirectory = $InstallDir
    $Shortcut.Description = 'Private, local-first desktop assistant'
    $Shortcut.Save()
    Write-Host 'Created Start Menu shortcut.' -ForegroundColor Gray
} catch {}

Write-Host ''
Write-Host ('DeskPilot successfully installed to: ' + $InstallDir + '\' + $BinaryName) -ForegroundColor Green
Write-Host 'You can now run deskpilot from PowerShell or launch it from Start Menu!' -ForegroundColor Yellow
Write-Host '=========================================' -ForegroundColor Cyan

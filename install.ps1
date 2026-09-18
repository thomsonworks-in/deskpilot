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
$Downloaded = $false
try {
    Write-Host ('Downloading DeskPilot (' + $ZipName + ')...') -ForegroundColor Cyan
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempZip
    Write-Host 'Extracting...' -ForegroundColor Gray
    Expand-Archive -Path $TempZip -DestinationPath $InstallDir -Force
    Remove-Item -Force $TempZip
    $Downloaded = $true
} catch {
    Write-Host "GitHub release asset download failed. Checking for local binary..." -ForegroundColor Yellow
    $LocalExe = if ($PSScriptRoot) { Join-Path $PSScriptRoot 'target\debug\deskpilot.exe' } else { '.\target\debug\deskpilot.exe' }
    if (Test-Path $LocalExe) {
        Copy-Item -Path $LocalExe -Destination (Join-Path $InstallDir $BinaryName) -Force
        $Downloaded = $true
        Write-Host "Installed local DeskPilot binary." -ForegroundColor Green
    } else {
        throw "Could not download deskpilot release asset ($ZipName): $($_.Exception.Message)"
    }
}

# Create short 'dp' command runner for CMD and PowerShell
$DpCmd = Join-Path $InstallDir 'dp.cmd'
'@echo off' + "`r`n" + 'powershell -NoProfile -ExecutionPolicy Bypass -Command "if (Get-Process deskpilot -ErrorAction SilentlyContinue) { Start-Process ''http://127.0.0.1:31415'' } else { Start-Process -FilePath ''%~dp0deskpilot.exe'' -WindowStyle Hidden; Start-Sleep -Milliseconds 600; Start-Process ''http://127.0.0.1:31415'' }"' | Out-File -FilePath $DpCmd -Encoding ascii -Force

$DpPs1 = Join-Path $InstallDir 'dp.ps1'
'if (Get-Process deskpilot -ErrorAction SilentlyContinue) { Start-Process ''http://127.0.0.1:31415'' } else { Start-Process -FilePath (Join-Path $PSScriptRoot ''deskpilot.exe'') -WindowStyle Hidden; Start-Sleep -Milliseconds 600; Start-Process ''http://127.0.0.1:31415'' }' | Out-File -FilePath $DpPs1 -Encoding ascii -Force

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

# Launch DeskPilot daemon in background and open Web Studio
$ExePath = Join-Path $InstallDir $BinaryName
Write-Host 'Launching DeskPilot Headless Daemon & Studio...' -ForegroundColor Cyan
Start-Process -FilePath $ExePath -WindowStyle Hidden
Start-Sleep -Milliseconds 600
Start-Process 'http://127.0.0.1:31415'

Write-Host ''
Write-Host '=========================================' -ForegroundColor Cyan
Write-Host '  DeskPilot Studio is Live & Running!    ' -ForegroundColor Green
Write-Host '  Web Studio: http://127.0.0.1:31415     ' -ForegroundColor Cyan
Write-Host '  Ultra-Short Run Command: dp            ' -ForegroundColor Yellow
Write-Host '=========================================' -ForegroundColor Cyan

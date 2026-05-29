#Requires -Version 5.1
# Install the winget sm-plugin into the tedge sm-plugins directory.
# Run as Administrator.

$destination = 'C:\ProgramData\tedge\sm-plugins'

if (-not (Test-Path $destination)) {
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Copy-Item -Path (Join-Path $scriptDir 'winget.ps1') -Destination $destination -Force

Write-Output "Installed winget.ps1 to $destination"
Write-Output "Verify the Microsoft.WinGet.Client module is available:"
Write-Output "  Install-Module Microsoft.WinGet.Client -Force -AllowClobber"

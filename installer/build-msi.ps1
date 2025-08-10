# Build script for SpaceCat MSI installer
# Requires WiX Toolset v3.11 or later

param(
    [string]$Version = "0.1.0",
    [string]$Configuration = "Release",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# Set paths
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$InstallerDir = $PSScriptRoot
$TargetDir = Join-Path $ProjectRoot "target\$Configuration"
$OutputDir = Join-Path $InstallerDir "output"

Write-Host "SpaceCat MSI Builder" -ForegroundColor Cyan
Write-Host "===================" -ForegroundColor Cyan
Write-Host ""

# Build the Rust project if not skipped
if (-not $SkipBuild) {
    Write-Host "Building SpaceCat in $Configuration mode..." -ForegroundColor Yellow
    Push-Location $ProjectRoot
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed"
        }
    }
    finally {
        Pop-Location
    }
} else {
    Write-Host "Skipping Rust build (using existing binary)" -ForegroundColor Yellow
}

# Check if binary exists
$ExePath = Join-Path $TargetDir "spacecat.exe"
if (-not (Test-Path $ExePath)) {
    throw "SpaceCat executable not found at: $ExePath"
}

Write-Host "Found executable: $ExePath" -ForegroundColor Green

# Create output directory
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
}

# Create config example if it doesn't exist
$ConfigExample = Join-Path $InstallerDir "config.example.json"
if (-not (Test-Path $ConfigExample)) {
    Write-Host "Creating config.example.json..." -ForegroundColor Yellow
    @{
        api = @{
            base_url = "http://192.168.0.82:1888"
            timeout_seconds = 30
            retry_attempts = 3
        }
        logging = @{
            level = "info"
            enable_file_logging = $false
            log_file = "spacecat.log"
        }
        discord = @{
            webhook_url = "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN"
            enabled = $true
            image_cooldown_seconds = 60
        }
    } | ConvertTo-Json -Depth 3 | Set-Content $ConfigExample
}

# Copy README to installer directory
$ReadmeSrc = Join-Path $ProjectRoot "README.md"
$ReadmeDst = Join-Path $InstallerDir "README.md"
if (Test-Path $ReadmeSrc) {
    Copy-Item $ReadmeSrc $ReadmeDst -Force
} else {
    # Create a basic README if it doesn't exist
    Write-Host "Creating README.md..." -ForegroundColor Yellow
    @"
# SpaceCat

SpaceCat is an astronomical observation system that interfaces with NINA Advanced API for monitoring and posting to Discord.

## Features
- Real-time event monitoring
- Image history management
- Discord integration
- Windows Service support
- Mount and sequence tracking

## Configuration
Copy `config.example.json` to `C:\ProgramData\SpaceCat\config.json` and edit with your settings.

## Usage
Run `spacecat --help` for available commands.

For more information, visit the project repository.
"@ | Set-Content $ReadmeDst
}

# Create LICENSE.rtf from the actual project LICENSE file
$LicenseRtf = Join-Path $InstallerDir "LICENSE.rtf"
$LicenseSrc = Join-Path $ProjectRoot "LICENSE"
$shouldUpdateLicense = $false
if (-not (Test-Path $LicenseRtf)) {
    $shouldUpdateLicense = $true
} elseif (Test-Path $LicenseSrc) {
    $srcModified = (Get-Item $LicenseSrc).LastWriteTime
    $rtfModified = (Get-Item $LicenseRtf).LastWriteTime
    if ($srcModified -gt $rtfModified) {
        $shouldUpdateLicense = $true
    }
}

if ($shouldUpdateLicense) {
    if (Test-Path $LicenseSrc) {
        Write-Host "Converting LICENSE to RTF format..." -ForegroundColor Yellow
        $licenseContent = Get-Content $LicenseSrc -Raw
        # Convert to RTF format
        $rtfContent = @"
{\rtf1\ansi\deff0 {\fonttbl{\f0\fswiss\fcharset0 Arial;}}
\f0\fs20
"@
        # Convert line breaks and escape special RTF characters
        $licenseContent = $licenseContent -replace "\r\n", "\par\r\n" -replace "\n", "\par\r\n"
        $licenseContent = $licenseContent -replace "\\", "\\\\"
        $licenseContent = $licenseContent -replace "\{", "\\{"
        $licenseContent = $licenseContent -replace "\}", "\\}"
        
        $rtfContent += $licenseContent + "\par\r\n}"
        $rtfContent | Set-Content $LicenseRtf
    } else {
        Write-Host "Warning: LICENSE file not found at $LicenseSrc" -ForegroundColor Yellow
        Write-Host "Creating fallback LICENSE.rtf..." -ForegroundColor Yellow
        @"
{\rtf1\ansi\deff0 {\fonttbl{\f0 Times New Roman;}}
\f0\fs20
Apache License 2.0\par
\par
Copyright 2025 SpaceCat Contributors\par
\par
Licensed under the Apache License, Version 2.0 (the "License");\par
you may not use this file except in compliance with the License.\par
You may obtain a copy of the License at\par
\par
    http://www.apache.org/licenses/LICENSE-2.0\par
\par
Unless required by applicable law or agreed to in writing, software\par
distributed under the License is distributed on an "AS IS" BASIS,\par
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.\par
See the License for the specific language governing permissions and\par
limitations under the License.\par
}
"@ | Set-Content $LicenseRtf
    }
}

# Copy executable to installer directory
$ExeDst = Join-Path $InstallerDir "spacecat.exe"
Copy-Item $ExePath $ExeDst -Force

# Check for WiX Toolset
$WixPath = $null
$PossiblePaths = @(
    "${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin",
    "${env:ProgramFiles}\WiX Toolset v3.14\bin",
    "${env:ProgramFiles(x86)}\WiX Toolset v3.11\bin",
    "${env:ProgramFiles}\WiX Toolset v3.11\bin",
    "${env:WIX}\bin"
)

foreach ($path in $PossiblePaths) {
    if (Test-Path $path) {
        $WixPath = $path
        break
    }
}

if (-not $WixPath) {
    Write-Host "WiX Toolset not found. Please install WiX Toolset v3.14 (recommended) or v3.11." -ForegroundColor Red
    Write-Host "Download from: https://github.com/wixtoolset/wix3/releases" -ForegroundColor Yellow
    Write-Host "v3.14: https://github.com/wixtoolset/wix3/releases/download/wix314rtm/wix314.exe" -ForegroundColor Yellow
    Write-Host "v3.11: https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311.exe" -ForegroundColor Yellow
    exit 1
}

Write-Host "Found WiX Toolset at: $WixPath" -ForegroundColor Green

# Set WiX tool paths
$Candle = Join-Path $WixPath "candle.exe"
$Light = Join-Path $WixPath "light.exe"

# Compile WiX source
Write-Host "Compiling WiX source..." -ForegroundColor Yellow
$WixObj = Join-Path $OutputDir "spacecat.wixobj"

& $Candle `
    -dSourceDir="$InstallerDir" `
    -dVersion="$Version" `
    -out "$WixObj" `
    "$InstallerDir\spacecat.wxs" `
    -ext WixUIExtension `
    -ext WixUtilExtension

if ($LASTEXITCODE -ne 0) {
    throw "WiX compilation failed"
}

# Link to create MSI
Write-Host "Creating MSI package..." -ForegroundColor Yellow
$MsiPath = Join-Path $OutputDir "SpaceCat-$Version-x64.msi"

& $Light `
    -out "$MsiPath" `
    "$WixObj" `
    -ext WixUIExtension `
    -ext WixUtilExtension `
    -cultures:en-US `
    -sice:ICE61 `
    -sice:ICE91

if ($LASTEXITCODE -ne 0) {
    throw "MSI creation failed"
}

# Clean up temporary files
Remove-Item $WixObj -Force -ErrorAction SilentlyContinue
Remove-Item "$OutputDir\*.wixpdb" -Force -ErrorAction SilentlyContinue

# Output summary
Write-Host ""
Write-Host "MSI package created successfully!" -ForegroundColor Green
Write-Host "Location: $MsiPath" -ForegroundColor Cyan
$FileInfo = Get-Item $MsiPath
Write-Host "Size: $([math]::Round($FileInfo.Length / 1MB, 2)) MB" -ForegroundColor Cyan
Write-Host ""
Write-Host "To install SpaceCat, run:" -ForegroundColor Yellow
Write-Host "  msiexec /i `"$MsiPath`"" -ForegroundColor White
Write-Host ""
Write-Host "For silent installation with service:" -ForegroundColor Yellow
Write-Host "  msiexec /i `"$MsiPath`" /quiet ADDLOCAL=ALL" -ForegroundColor White

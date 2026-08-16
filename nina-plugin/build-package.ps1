[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')]
    [string] $Version = '0.1.0.0',

    [ValidateNotNullOrEmpty()]
    [string] $InstallerUrl = '',

    [ValidateNotNullOrEmpty()]
    [string] $FeaturedImageUrl = 'https://raw.githubusercontent.com/theatrus/chatstronomy/main/assets/branding/chatstronomy-featured.png',

    [ValidateNotNullOrEmpty()]
    [string] $OutputDirectory = 'artifacts/nina-plugin',

    [string] $RuntimePath = '',

    [switch] $SkipRuntime,

    # Split the build so Authenticode signing has a seam. -StageOnly builds the
    # plugin DLL and the runtime into the package directory and stops; the
    # caller signs what is there; -PackageOnly then zips and writes the
    # manifest. Neither switch changes what a plain invocation does.
    #
    # The zip's SHA-256 goes into the manifest N.I.N.A. checks, so the archive
    # MUST be created after signing -- sign the contents afterwards and the
    # checksum no longer matches what was published.
    [switch] $StageOnly,

    [switch] $PackageOnly
)

if ($StageOnly -and $PackageOnly) {
    throw 'Specify at most one of -StageOnly and -PackageOnly.'
}

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$project = Join-Path $PSScriptRoot 'Chatstronomy.NINA/Chatstronomy.NINA.csproj'
$outputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory
} else {
    Join-Path $repositoryRoot $OutputDirectory
}

$archiveName = "Chatstronomy.NINA.$Version.zip"
if ([string]::IsNullOrWhiteSpace($InstallerUrl)) {
    $InstallerUrl = "https://github.com/theatrus/chatstronomy/releases/download/nina-v$Version/$archiveName"
}

$parsedInstallerUrl = $null
if (-not [Uri]::TryCreate($InstallerUrl, [UriKind]::Absolute, [ref] $parsedInstallerUrl) -or
    $parsedInstallerUrl.Scheme -notin @('http', 'https')) {
    throw 'InstallerUrl must be an absolute http:// or https:// URL.'
}

$parsedFeaturedImageUrl = $null
if (-not [Uri]::TryCreate($FeaturedImageUrl, [UriKind]::Absolute, [ref] $parsedFeaturedImageUrl) -or
    $parsedFeaturedImageUrl.Scheme -notin @('http', 'https')) {
    throw 'FeaturedImageUrl must be an absolute http:// or https:// URL.'
}

$buildDirectory = Join-Path $outputRoot 'build'
$packageDirectory = Join-Path $outputRoot 'package/Chatstronomy'
$archivePath = Join-Path $outputRoot $archiveName
$manifestPath = Join-Path $outputRoot "Chatstronomy.NINA.$Version.manifest.json"

# -PackageOnly resumes from a staged (and by then signed) package directory, so
# it must not wipe or rebuild it -- that would discard the signatures it exists
# to preserve.
if (-not $PackageOnly) {
    foreach ($directory in @($buildDirectory, $packageDirectory)) {
        if (Test-Path -LiteralPath $directory) {
            Remove-Item -LiteralPath $directory -Recurse -Force
        }
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
} elseif (-not (Test-Path -LiteralPath $packageDirectory)) {
    throw "-PackageOnly needs a staged package at $packageDirectory; run -StageOnly first."
}

foreach ($file in @($archivePath, $manifestPath)) {
    if (Test-Path -LiteralPath $file) {
        Remove-Item -LiteralPath $file -Force
    }
}

# Declared out here on purpose: -PackageOnly skips the block below, and
# Set-StrictMode turns a later read of an unset variable into a hard failure at
# the very end -- after the signing work is done and thrown away.
$packagedRuntime = $null

if (-not $PackageOnly) {

dotnet build $project `
    --configuration Release `
    --output $buildDirectory `
    -p:Version=$Version `
    -p:AssemblyVersion=$Version `
    -p:FileVersion=$Version
if ($LASTEXITCODE -ne 0) {
    throw "Chatstronomy N.I.N.A. plugin build failed with exit code $LASTEXITCODE."
}

$pluginDll = Join-Path $buildDirectory 'Chatstronomy.dll'
if (-not (Test-Path -LiteralPath $pluginDll)) {
    throw "Expected plugin assembly was not produced at $pluginDll."
}

Copy-Item -LiteralPath $pluginDll -Destination $packageDirectory

if (-not $SkipRuntime) {
    if ([string]::IsNullOrWhiteSpace($RuntimePath)) {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            throw 'cargo is required to build the bundled Chatstronomy runtime. Install Rust or pass -SkipRuntime.'
        }
        Push-Location $repositoryRoot
        try {
            cargo build --release --no-default-features
            if ($LASTEXITCODE -ne 0) {
                throw "Chatstronomy runtime build failed with exit code $LASTEXITCODE."
            }
        } finally {
            Pop-Location
        }
        $RuntimePath = Join-Path $repositoryRoot 'target/release/chatstronomy.exe'
    } elseif (-not [IO.Path]::IsPathRooted($RuntimePath)) {
        $RuntimePath = Join-Path $repositoryRoot $RuntimePath
    }

    if (-not (Test-Path -LiteralPath $RuntimePath -PathType Leaf)) {
        throw "Expected Chatstronomy runtime was not found at $RuntimePath."
    }
    $runtimeDirectory = Join-Path $packageDirectory 'runtime'
    New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
    $packagedRuntime = Join-Path $runtimeDirectory 'chatstronomy.exe'
    Copy-Item -LiteralPath $RuntimePath -Destination $packagedRuntime
}

} else {
    # Resuming from a staged package: report the runtime that staging produced,
    # so the summary means the same thing on both paths.
    $stagedRuntime = Join-Path $packageDirectory 'runtime/chatstronomy.exe'
    if (Test-Path -LiteralPath $stagedRuntime -PathType Leaf) {
        $packagedRuntime = $stagedRuntime
    }
}  # end of the build/stage phase skipped by -PackageOnly

if ($StageOnly) {
    Write-Host "Staged plugin package at $packageDirectory (not archived)."
    Write-Host 'Sign the contents, then re-run with -PackageOnly.'
    return
}

Compress-Archive -Path (Join-Path $packageDirectory '*') -DestinationPath $archivePath

$checksum = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
$versionParts = $Version.Split('.')
$manifest = [ordered]@{
    Name = 'Chatstronomy'
    Identifier = '5e7c25c4-f654-4e22-9e21-3127048221c0'
    Version = [ordered]@{
        Major = $versionParts[0]
        Minor = $versionParts[1]
        Patch = $versionParts[2]
        Build = $versionParts[3]
    }
    Author = 'Yann Ramin'
    Homepage = 'https://github.com/theatrus/chatstronomy'
    Repository = 'https://github.com/theatrus/chatstronomy'
    License = 'Apache-2.0'
    LicenseURL = 'https://github.com/theatrus/chatstronomy/blob/main/LICENSE'
    ChangelogURL = 'https://github.com/theatrus/chatstronomy/releases'
    Tags = @('discord', 'matrix', 'monitoring', 'remote')
    MinimumApplicationVersion = [ordered]@{
        Major = '3'
        Minor = '2'
        Patch = '0'
        Build = '9001'
    }
    Descriptions = [ordered]@{
        ShortDescription = 'Bridge NINA with Discord and Matrix, supporting bot slash commands for control'
        LongDescription = 'Routes N.I.N.A. status, events, images, and approved commands through Discord and Matrix chat. Includes a supervised on-machine runtime with Advanced API polling, plus multi-system Remote and native Direct integration paths.'
        FeaturedImageURL = $FeaturedImageUrl
        ScreenshotURL = ''
        AltScreenshotURL = ''
    }
    Installer = [ordered]@{
        URL = $InstallerUrl
        Type = 'ARCHIVE'
        Checksum = $checksum
        ChecksumType = 'SHA256'
    }
    Channel = 'Beta'
}

$json = $manifest | ConvertTo-Json -Depth 10
[IO.File]::WriteAllText($manifestPath, $json, [Text.UTF8Encoding]::new($false))

[pscustomobject]@{
    Archive = $archivePath
    Manifest = $manifestPath
    Checksum = $checksum
    InstallerUrl = $InstallerUrl
    Runtime = $packagedRuntime
}

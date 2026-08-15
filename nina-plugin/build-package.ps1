[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')]
    [string] $Version = '0.1.0.0',

    [ValidateNotNullOrEmpty()]
    [string] $InstallerUrl = '',

    [ValidateNotNullOrEmpty()]
    [string] $FeaturedImageUrl = 'https://raw.githubusercontent.com/theatrus/spacecat/main/assets/branding/spacecat-nina-plugin-featured.png',

    [ValidateNotNullOrEmpty()]
    [string] $OutputDirectory = 'artifacts/nina-plugin'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$project = Join-Path $PSScriptRoot 'SpaceCat.NINA/SpaceCat.NINA.csproj'
$outputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory
} else {
    Join-Path $repositoryRoot $OutputDirectory
}

$archiveName = "SpaceCat.NINA.$Version.zip"
if ([string]::IsNullOrWhiteSpace($InstallerUrl)) {
    $InstallerUrl = "https://github.com/theatrus/spacecat/releases/download/nina-v$Version/$archiveName"
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
$packageDirectory = Join-Path $outputRoot 'package/SpaceCat'
$archivePath = Join-Path $outputRoot $archiveName
$manifestPath = Join-Path $outputRoot "SpaceCat.NINA.$Version.manifest.json"

foreach ($directory in @($buildDirectory, $packageDirectory)) {
    if (Test-Path -LiteralPath $directory) {
        Remove-Item -LiteralPath $directory -Recurse -Force
    }
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
}

foreach ($file in @($archivePath, $manifestPath)) {
    if (Test-Path -LiteralPath $file) {
        Remove-Item -LiteralPath $file -Force
    }
}

dotnet build $project `
    --configuration Release `
    --output $buildDirectory `
    -p:Version=$Version `
    -p:AssemblyVersion=$Version `
    -p:FileVersion=$Version
if ($LASTEXITCODE -ne 0) {
    throw "SpaceCat N.I.N.A. plugin build failed with exit code $LASTEXITCODE."
}

$pluginDll = Join-Path $buildDirectory 'SpaceCat.dll'
if (-not (Test-Path -LiteralPath $pluginDll)) {
    throw "Expected plugin assembly was not produced at $pluginDll."
}

Copy-Item -LiteralPath $pluginDll -Destination $packageDirectory
Compress-Archive -LiteralPath (Join-Path $packageDirectory 'SpaceCat.dll') -DestinationPath $archivePath

$checksum = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
$versionParts = $Version.Split('.')
$manifest = [ordered]@{
    Name = 'SpaceCat'
    Identifier = '5e7c25c4-f654-4e22-9e21-3127048221c0'
    Version = [ordered]@{
        Major = $versionParts[0]
        Minor = $versionParts[1]
        Patch = $versionParts[2]
        Build = $versionParts[3]
    }
    Author = 'Yann Ramin'
    Homepage = 'https://github.com/theatrus/spacecat'
    Repository = 'https://github.com/theatrus/spacecat'
    License = 'Apache-2.0'
    LicenseURL = 'https://github.com/theatrus/spacecat/blob/main/LICENSE'
    ChangelogURL = 'https://github.com/theatrus/spacecat/releases'
    Tags = @('discord', 'matrix', 'monitoring', 'remote')
    MinimumApplicationVersion = [ordered]@{
        Major = '3'
        Minor = '2'
        Patch = '0'
        Build = '9001'
    }
    Descriptions = [ordered]@{
        ShortDescription = 'Connect N.I.N.A. to local or hosted SpaceCat services.'
        LongDescription = 'Connects N.I.N.A. to SpaceCat for Discord and Matrix status, events, images, and approved commands. Supports simple on-machine Local mode, multi-system Remote mode, and compatibility with Advanced API deployments.'
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
}

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $PluginRuntimePath,

    [Parameter(Mandatory)]
    [string] $FullRuntimePath,

    [Parameter(Mandatory)]
    [string] $PluginContractsPath,

    [Parameter(Mandatory)]
    [ValidatePattern('^v\d+\.\d+\.\d+$')]
    [string] $Tag,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $Commit,

    [string] $OutputPath = 'chatstronomy-runtime-manifest.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-AssetPath([string] $Path, [string] $Description) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Description was not found at $Path."
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Read-ArtifactContract([string] $Path, [string] $ExpectedFlavor) {
    $json = & $Path artifact-contract | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "$Path artifact-contract failed with exit code $LASTEXITCODE."
    }

    try {
        $contract = $json | ConvertFrom-Json
    } catch {
        throw "$Path emitted an invalid artifact contract: $($_.Exception.Message)"
    }

    if ($contract.schema_version -ne 1 -or $contract.product -ne 'chatstronomy') {
        throw "$Path emitted an unsupported artifact contract."
    }
    if ($contract.flavor -ne $ExpectedFlavor) {
        throw "$Path is flavor '$($contract.flavor)', expected '$ExpectedFlavor'."
    }

    $releaseVersion = $Tag.Substring(1)
    if ($contract.runtime_version -ne $releaseVersion) {
        throw "$Path is runtime $($contract.runtime_version), expected $releaseVersion from $Tag."
    }

    $embeddedCommit = ([string] $contract.git_sha) -replace '-dirty$', ''
    if ([string]::IsNullOrWhiteSpace($embeddedCommit) -or
        -not $Commit.StartsWith($embeddedCommit, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Path embeds commit '$($contract.git_sha)', expected $Commit."
    }

    return $contract
}

function New-AssetRecord([string] $Path) {
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        name = $item.Name
        sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        size = $item.Length
    }
}

$pluginRuntime = Resolve-AssetPath $PluginRuntimePath 'Plugin runtime'
$fullRuntime = Resolve-AssetPath $FullRuntimePath 'Full runtime'
$pluginContracts = Resolve-AssetPath $PluginContractsPath 'Plugin contracts archive'

$pluginContract = Read-ArtifactContract $pluginRuntime 'plugin'
$fullContract = Read-ArtifactContract $fullRuntime 'full'

$pluginProtocols = $pluginContract.protocols | ConvertTo-Json -Depth 20 -Compress
$fullProtocols = $fullContract.protocols | ConvertTo-Json -Depth 20 -Compress
if ($pluginProtocols -ne $fullProtocols) {
    throw 'Full and plugin runtimes report different protocol contracts.'
}

$manifest = [ordered]@{
    schema_version = 1
    release = [ordered]@{
        repository = 'theatrus/chatstronomy'
        tag = $Tag
        version = $Tag.Substring(1)
        commit = $Commit.ToLowerInvariant()
    }
    protocols = $pluginContract.protocols
    assets = [ordered]@{
        plugin_runtime_windows_x64 = New-AssetRecord $pluginRuntime
        full_windows_x64 = New-AssetRecord $fullRuntime
        plugin_contracts = New-AssetRecord $pluginContracts
    }
}

$resolvedOutput = if ([IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path (Get-Location) $OutputPath
}
$outputDirectory = Split-Path -Parent $resolvedOutput
if (-not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
}

$json = $manifest | ConvertTo-Json -Depth 20
[IO.File]::WriteAllText($resolvedOutput, $json, [Text.UTF8Encoding]::new($false))

[pscustomobject]@{
    Manifest = $resolvedOutput
    Sha256 = (Get-FileHash -LiteralPath $resolvedOutput -Algorithm SHA256).Hash.ToLowerInvariant()
    RuntimeVersion = $manifest.release.version
    DirectProtocol = $manifest.protocols.direct.version
    PluginRuntimeProtocol = $manifest.protocols.plugin_runtime.version
}

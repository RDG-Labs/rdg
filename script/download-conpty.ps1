#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("x64", "arm64")]
    [string]$Architecture,

    [Parameter()]
    [string]$TargetDir = "target"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$conptyVersion = "1.24.260303001"
$releaseTag = "v1.24.10621.0"
$archiveName = "Microsoft.Windows.Console.ConPTY.$conptyVersion.nupkg"
$url = "https://github.com/microsoft/terminal/releases/download/$releaseTag/$archiveName"

New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
$targetDir = (Resolve-Path -LiteralPath $TargetDir).Path
$conptyTarget = Join-Path $targetDir "conpty.dll"
$openConsoleTarget = Join-Path $targetDir "OpenConsole.exe"

if ((Test-Path -LiteralPath $conptyTarget -PathType Leaf) -and
    (Test-Path -LiteralPath $openConsoleTarget -PathType Leaf)) {
    Write-Host "Windows terminal runtime already present in $targetDir"
    exit 0
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rdg-conpty-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $tempDir "$archiveName.zip"
$extractDir = Join-Path $tempDir "extracted"

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    Invoke-WebRequest -Uri $url -OutFile $archivePath
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir -Force

    $runtimeArchitecture = if ($Architecture -eq "x64") { "x64" } else { "arm64" }
    $conptySource = Join-Path $extractDir "runtimes\win-$Architecture\native\conpty.dll"
    $openConsoleSource = Join-Path $extractDir "build\native\runtimes\$runtimeArchitecture\OpenConsole.exe"

    foreach ($source in @($conptySource, $openConsoleSource)) {
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "ConPTY package is missing expected file: $source"
        }
    }

    Copy-Item -LiteralPath $conptySource -Destination $conptyTarget -Force
    Copy-Item -LiteralPath $openConsoleSource -Destination $openConsoleTarget -Force
}
finally {
    Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}

foreach ($requiredFile in @($conptyTarget, $openConsoleTarget)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "Failed to prepare Windows terminal runtime: $requiredFile"
    }
}

Write-Host "Prepared Windows terminal runtime in $targetDir"

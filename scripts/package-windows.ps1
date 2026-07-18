param(
    [ValidateSet("x86_64-pc-windows-msvc")]
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root "dist"
$package = Join-Path $dist "SeeUsage-windows-x86_64"
$distFull = [System.IO.Path]::GetFullPath($dist)
$packageFull = [System.IO.Path]::GetFullPath($package)
if (-not $packageFull.StartsWith($distFull + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to package outside the dist directory: $packageFull"
}

Push-Location $root
try {
    & "$PSScriptRoot\build-windows.ps1" -Target $Target
    if (Test-Path -LiteralPath $package) {
        Remove-Item -Recurse -Force -LiteralPath $package
    }
    New-Item -ItemType Directory -Force -Path $package | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $package "assets") | Out-Null
    Copy-Item -LiteralPath "target\$Target\release\SeeUsage.exe" -Destination $package
    Copy-Item -LiteralPath "scripts\install-windows.ps1" -Destination (Join-Path $package "Install-SeeUsage.ps1")
    Copy-Item -LiteralPath "assets\seeusage-preview.png" -Destination (Join-Path $package "assets")
    Copy-Item -LiteralPath "README.md", "README.zh-CN.md", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_DEPENDENCIES.md" -Destination $package

    $archive = "$package.zip"
    if (Test-Path -LiteralPath $archive) {
        Remove-Item -Force -LiteralPath $archive
    }
    Compress-Archive -Path "$package\*" -DestinationPath $archive
    $checksumPath = "$archive.sha256"
    $checksum = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
    $checksumLine = "$checksum  $([System.IO.Path]::GetFileName($archive))`n"
    [System.IO.File]::WriteAllText(
        $checksumPath,
        $checksumLine,
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-Output $archive
    Write-Output $checksumPath
}
finally {
    Pop-Location
}

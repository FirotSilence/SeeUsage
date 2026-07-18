[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$outputPath = Join-Path $root "THIRD_PARTY_DEPENDENCIES.md"
$targets = [ordered]@{
    "x86_64-pc-windows-msvc" = "Windows x86_64"
    "aarch64-apple-darwin" = "macOS arm64"
}
$packages = @{}

Push-Location $root
try {
    foreach ($entry in $targets.GetEnumerator()) {
        $lines = & cargo tree --locked --target $entry.Key -e "normal,build" --prefix none --format "{p}|{l}|{r}"
        if ($LASTEXITCODE -ne 0) {
            throw "cargo tree failed for $($entry.Key) with exit code $LASTEXITCODE"
        }

        foreach ($line in $lines) {
            $parts = $line -split "\|", 3
            if ($parts.Count -ne 3) {
                continue
            }
            $package = $parts[0] -replace " \(\*\)$", "" -replace " \(proc-macro\)$", ""
            if ($package -notmatch "^(?<name>\S+) v(?<version>\S+)$") {
                continue
            }
            if ($Matches.name -eq "seeusage") {
                continue
            }

            $key = "$($Matches.name)@$($Matches.version)"
            if (-not $packages.ContainsKey($key)) {
                $packages[$key] = [ordered]@{
                    Name = $Matches.name
                    Version = $Matches.version
                    License = $parts[1]
                    Repository = $parts[2]
                    Targets = [System.Collections.Generic.SortedSet[string]]::new()
                }
            }
            [void]$packages[$key].Targets.Add($entry.Value)
        }
    }
}
finally {
    Pop-Location
}

$content = [System.Text.StringBuilder]::new()
[void]$content.AppendLine("# Third-party dependency inventory")
[void]$content.AppendLine()
[void]$content.AppendLine("This generated inventory covers normal and build dependencies selected from")
[void]$content.AppendLine('`Cargo.lock` for the supported release targets. License expressions come from')
[void]$content.AppendLine("crate package metadata.")
[void]$content.AppendLine()
[void]$content.AppendLine("| Crate | Version | License | Release targets | Source |")
[void]$content.AppendLine("| --- | --- | --- | --- | --- |")
foreach ($key in $packages.Keys | Sort-Object) {
    $package = $packages[$key]
    $repository = $package.Repository
    if ([string]::IsNullOrWhiteSpace($repository)) {
        $repository = "https://crates.io/crates/$($package.Name)/$($package.Version)"
    }
    $targetsText = [string]::Join("<br>", $package.Targets)
    [void]$content.AppendLine("| $($package.Name) | $($package.Version) | $($package.License) | $targetsText | [link]($repository) |")
}

[System.IO.File]::WriteAllText(
    $outputPath,
    $content.ToString(),
    [System.Text.UTF8Encoding]::new($false)
)
Write-Output $outputPath

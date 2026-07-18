[CmdletBinding()]
param(
    [ValidateSet("x86_64-pc-windows-msvc")]
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$separator = [string][char]0x1f
$hadEncodedFlags = Test-Path Env:CARGO_ENCODED_RUSTFLAGS
$previousEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
$userProfile = [Environment]::GetFolderPath("UserProfile")
$cargoHome = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
    $env:CARGO_HOME
}
elseif (-not [string]::IsNullOrWhiteSpace($userProfile)) {
    Join-Path $userProfile ".cargo"
}
$rustupHome = if (-not [string]::IsNullOrWhiteSpace($env:RUSTUP_HOME)) {
    $env:RUSTUP_HOME
}
elseif (-not [string]::IsNullOrWhiteSpace($userProfile)) {
    Join-Path $userProfile ".rustup"
}

$remapFlags = @()
if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
    $remapFlags += "--remap-path-prefix=$userProfile=user-home"
}
if (-not [string]::IsNullOrWhiteSpace($cargoHome)) {
    $remapFlags += "--remap-path-prefix=$cargoHome=cargo-home"
}
if (-not [string]::IsNullOrWhiteSpace($rustupHome)) {
    $remapFlags += "--remap-path-prefix=$rustupHome=rustup-home"
}
$remapFlags += "--remap-path-prefix=$root=seeusage-src"
$encodedRemapFlags = $remapFlags -join $separator
if ([string]::IsNullOrEmpty($previousEncodedFlags)) {
    $env:CARGO_ENCODED_RUSTFLAGS = $encodedRemapFlags
}
else {
    $env:CARGO_ENCODED_RUSTFLAGS = "$previousEncodedFlags$separator$encodedRemapFlags"
}

Push-Location $root
try {
    cargo build --release --locked --target $Target
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}
finally {
    if ($hadEncodedFlags) {
        $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags
    }
    else {
        Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    }
    Pop-Location
}

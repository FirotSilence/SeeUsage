[CmdletBinding()]
param(
    [ValidateSet("x86_64-pc-windows-msvc")]
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$NoBuild,
    [switch]$NoStart
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$portableBinary = Join-Path $PSScriptRoot "SeeUsage.exe"
$builtBinary = Join-Path $root "target\$Target\release\SeeUsage.exe"

if (Test-Path -LiteralPath $portableBinary) {
    $sourceBinary = $portableBinary
}
else {
    if (-not $NoBuild) {
        Push-Location $root
        try {
            & "$PSScriptRoot\build-windows.ps1" -Target $Target
        }
        finally {
            Pop-Location
        }
    }
    $sourceBinary = $builtBinary
}

if (-not (Test-Path -LiteralPath $sourceBinary -PathType Leaf)) {
    throw "SeeUsage.exe was not found at $sourceBinary"
}
if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
    throw "LOCALAPPDATA is unavailable"
}

$installDirectory = Join-Path $env:LOCALAPPDATA "Programs\SeeUsage"
$dataDirectory = Join-Path $env:LOCALAPPDATA "SeeUsage"
$destination = Join-Path $installDirectory "SeeUsage.exe"
$running = Get-Process -Name "SeeUsage" -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ([System.IO.Path]::GetFullPath($_.Path) -eq [System.IO.Path]::GetFullPath($destination)) }
if ($running) {
    throw "Quit the installed SeeUsage instance before updating it"
}

$currentUser = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [System.Security.Principal.SecurityIdentifier]::new("S-1-5-18")
$administrators = [System.Security.Principal.SecurityIdentifier]::new("S-1-5-32-544")
$inheritance = [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor
    [System.Security.AccessControl.InheritanceFlags]::ObjectInherit
$propagation = [System.Security.AccessControl.PropagationFlags]::None
$allow = [System.Security.AccessControl.AccessControlType]::Allow
$fullControl = [System.Security.AccessControl.FileSystemRights]::FullControl
foreach ($directory in @($installDirectory, $dataDirectory)) {
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $acl = [System.Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner($currentUser)
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($identity in @($currentUser, $system, $administrators)) {
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
            $identity,
            $fullControl,
            $inheritance,
            $propagation,
            $allow
        )
        [void]$acl.AddAccessRule($rule)
    }
    $directoryInfo = Get-Item -LiteralPath $directory
    [System.IO.FileSystemAclExtensions]::SetAccessControl($directoryInfo, $acl)
}

$staged = Join-Path $installDirectory "SeeUsage.exe.new"
Copy-Item -Force -LiteralPath $sourceBinary -Destination $staged
Move-Item -Force -LiteralPath $staged -Destination $destination

Write-Output $destination
if (-not $NoStart) {
    Start-Process -FilePath $destination -WindowStyle Hidden
}

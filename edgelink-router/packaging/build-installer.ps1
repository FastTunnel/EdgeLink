param(
    [ValidateSet("dev", "release")]
    [string]$Profile = "dev"
)

$ErrorActionPreference = "Stop"

$ProjectDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RepoRoot   = (Resolve-Path (Join-Path $ProjectDir "..\..")).Path
$DistDir    = Join-Path $RepoRoot "dist"
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

$CargoToml = Get-Content (Join-Path $ProjectDir "Cargo.toml") -Raw
if ($CargoToml -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    throw "Could not parse version from Cargo.toml"
}

Push-Location $ProjectDir
try {
    Get-Process -Name "edgelink-router" -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-Process -Name "windows-route-client" -ErrorAction SilentlyContinue | Stop-Process -Force

    if ($Profile -eq "release") {
        Write-Host "[1/2] Building release binaries (edgelink-router, edgelink-service, edgelink-setup)..." -ForegroundColor Cyan
        cargo build --release --bin edgelink-router --bin edgelink-setup
        $TargetDir = Join-Path $ProjectDir "target\release"
        $ServiceExe = Join-Path $TargetDir "edgelink-service.exe"
        cargo build --release --bin edgelink-service
    } else {
        Write-Host "[1/2] Building dev binaries (edgelink-router, edgelink-service, edgelink-setup)..." -ForegroundColor Cyan
        cargo build --bin edgelink-router --bin edgelink-setup
        $TargetDir = Join-Path $ProjectDir "target\debug"
        $ServiceExe = Join-Path $TargetDir "edgelink-service.exe"
        cmd /c "cargo build --bin edgelink-service >nul 2>nul"
        if (-not (Test-Path $ServiceExe)) {
            cargo build --bin edgelink-service
        }
    }

    $ClientExe = Join-Path $TargetDir "edgelink-router.exe"
    $SetupExe  = Join-Path $TargetDir "edgelink-setup.exe"
    $IconIco   = Join-Path $ProjectDir "assets\app.ico"
    $OutputExe = Join-Path $DistDir "edgelink-router_${Version}_x64_setup.exe"

    if (-not (Test-Path $ClientExe))  { throw "Missing binary: $ClientExe" }
    if (-not (Test-Path $ServiceExe)) { throw "Missing binary: $ServiceExe" }
    if (-not (Test-Path $SetupExe))   { throw "Missing binary: $SetupExe" }

    Write-Host "[2/2] Packing standalone EdgeLink installer executable..." -ForegroundColor Cyan
    if (Test-Path $OutputExe) { Remove-Item $OutputExe -Force }
    $proc = Start-Process -FilePath $SetupExe -ArgumentList @("--pack", $ClientExe, $ServiceExe, $IconIco, $OutputExe) -Wait -PassThru -NoNewWindow
    if ($proc.ExitCode -ne 0 -or -not (Test-Path $OutputExe)) {
        throw "Failed to create installer executable (exit code: $($proc.ExitCode))"
    }

    $SizeMB = [math]::Round((Get-Item $OutputExe).Length / 1MB, 2)
    Write-Host "Created installer: $OutputExe ($SizeMB MB)" -ForegroundColor Green
} finally {
    Pop-Location
}

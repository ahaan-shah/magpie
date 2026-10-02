# Builds Magpie for Windows: an x64 and an ARM64 zip, plus one installer
# (Magpie-windows-setup.exe) that installs the right build for the PC.
#
#   pwsh scripts/package-windows.ps1 [version]
#
# Needs the MSVC toolchain with ARM64 build tools, both Rust targets, and
# Inno Setup 6.3+ (choco install innosetup).
param([string]$Version = "")
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

# A tag like v0.2.0 gives the version; anything else (a branch name on a
# manual run) falls back to Cargo.toml.
$Version = $Version.TrimStart('v')
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}

$targets = [ordered]@{ 'x64' = 'x86_64-pc-windows-msvc'; 'arm64' = 'aarch64-pc-windows-msvc' }
New-Item -ItemType Directory -Force dist | Out-Null

foreach ($arch in $targets.Keys) {
    $target = $targets[$arch]
    cargo build --profile dist --locked --target $target -p magpie-finance
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $target" }

    $stage = "dist\Magpie-windows-$arch"
    Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
    New-Item -ItemType Directory $stage | Out-Null
    Copy-Item "target\$target\dist\magpie.exe" $stage
    Copy-Item LICENSE "$stage\LICENSE.txt"
    Copy-Item README.md $stage
    Compress-Archive -Path "$stage\*" -DestinationPath "dist\Magpie-windows-$arch.zip" -Force
    Remove-Item -Recurse -Force $stage
    Write-Host "Built dist\Magpie-windows-$arch.zip"
}

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
if (-not (Test-Path $iscc)) { throw "Inno Setup 6 not found (choco install innosetup)" }

$root = (Get-Location).Path
& $iscc /Qp "/DVersion=$Version" `
    "/DSrcX64=$root\target\x86_64-pc-windows-msvc\dist\magpie.exe" `
    "/DSrcArm64=$root\target\aarch64-pc-windows-msvc\dist\magpie.exe" `
    "/DOutDir=$root\dist" `
    packaging\windows\magpie.iss
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
Write-Host "Built dist\Magpie-windows-setup.exe"

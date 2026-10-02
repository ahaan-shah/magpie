# End-to-end check of the Windows installer on a clean machine (CI):
# install silently, run the CLI, open the real app and screenshot every page,
# upgrade in place, uninstall, and make sure user data survives.
#
#   pwsh scripts/smoke-windows.ps1 dist\Magpie-windows-setup.exe <screenshots dir>
param(
    [Parameter(Mandatory)] [string]$Setup,
    [Parameter(Mandatory)] [string]$Shots
)
$ErrorActionPreference = 'Stop'
$Setup = (Resolve-Path $Setup).Path
New-Item -ItemType Directory -Force $Shots | Out-Null
$Shots = (Resolve-Path $Shots).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("magpie-smoke-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
$dir = Join-Path $work 'app'
$data = Join-Path $work 'data'

function Install-Magpie {
    $log = Join-Path $work 'setup.log'
    $p = Start-Process $Setup -Wait -PassThru -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', "/DIR=$dir", "/LOG=$log"
    if ($p.ExitCode -ne 0) { Get-Content $log -ErrorAction SilentlyContinue; throw "setup exited with $($p.ExitCode)" }
    if (-not (Test-Path "$dir\magpie.exe")) { throw "magpie.exe wasn't installed" }
}

Write-Host "== install"
Install-Magpie
$info = (Get-Item "$dir\magpie.exe").VersionInfo
Write-Host "installed $($info.ProductName) $($info.FileVersion)"
if ($info.ProductName -ne 'Magpie') { throw "magpie.exe is missing its version resource" }

Write-Host "== cli"
$out = Join-Path $work 'version.txt'
$p = Start-Process "$dir\magpie.exe" -ArgumentList '--version' -Wait -PassThru -NoNewWindow -RedirectStandardOutput $out
$version = (Get-Content $out -Raw).Trim()
Write-Host "magpie --version: $version"
if ($p.ExitCode -ne 0 -or $version -notmatch '^magpie \d+\.\d+\.\d+') { throw "--version failed: '$version'" }

Write-Host "== gui tour"
$env:MAGPIE_TOUR = $Shots
$env:MAGPIE_DATA_DIR = $data
$p = Start-Process "$dir\magpie.exe" -ArgumentList '--demo' -PassThru
$null = $p.Handle  # keeps ExitCode readable after the process exits
if (-not $p.WaitForExit(240000)) { $p.Kill(); throw "the tour didn't finish in 4 minutes" }
Remove-Item Env:MAGPIE_TOUR
$log = Join-Path $data 'magpie-diagnostics.log'
if (Test-Path $log) { Write-Host "-- diagnostics log"; Get-Content $log }
$pngs = @(Get-ChildItem $Shots -Filter *.png)
Write-Host "tour saved $($pngs.Count) screenshots, exit code $($p.ExitCode)"
if ($p.ExitCode -ne 0) { throw "magpie exited with $($p.ExitCode)" }
if ($pngs.Count -lt 15) { throw "expected at least 15 screenshots" }
foreach ($png in $pngs) { if ($png.Length -lt 20000) { throw "$($png.Name) looks blank ($($png.Length) bytes)" } }

Write-Host "== real data folder survives upgrade and uninstall"
New-Item -ItemType File -Force (Join-Path $data 'keep-me.txt') | Out-Null
Install-Magpie   # upgrade in place over the existing install
$uninstaller = Get-ChildItem $dir -Filter 'unins*.exe' | Select-Object -First 1
if (-not $uninstaller) { throw "no uninstaller found" }
$p = Start-Process $uninstaller.FullName -Wait -PassThru -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
# The uninstaller copies itself to %TEMP% and returns early; give it a moment.
for ($i = 0; $i -lt 30 -and (Test-Path "$dir\magpie.exe"); $i++) { Start-Sleep -Seconds 1 }
if (Test-Path "$dir\magpie.exe") { throw "uninstall left magpie.exe behind" }
if (-not (Test-Path (Join-Path $data 'keep-me.txt'))) { throw "uninstall touched user data" }

Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
Write-Host "== all good"

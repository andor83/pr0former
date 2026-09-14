# Native Windows desktop build. macOS and Linux use build.sh.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$Bundles = ''
$Jobs = if ($env:PR0_BUILD_JOBS) { $env:PR0_BUILD_JOBS } else { '4' }
$InstallDeps = $false
$SigningMode = 'default'

function Show-Usage {
  @'
Usage: .\build.ps1 [--bundles nsis|msi|nsis,msi] [--jobs N] [--install-deps]
                   [--unsigned | --no-prompt]

Build the native Windows pr0former Tauri app for this machine (x86-64).
Default bundles: NSIS and MSI. The build does not launch the app or install services.
It builds the Rust server, Vue interface, and pinned FFmpeg executable.

Required: 64-bit Windows, Rust 1.88+, Node.js 22.12+, Python 3, CMake,
Visual Studio 2022 C++ Build Tools, and MSYS2 UCRT64 with GCC and make.
Missing dependencies are reported with exact winget/MSYS2 commands. In a terminal
the script offers to install them; --install-deps accepts installation automatically.
Reopen PowerShell after a system tool install if its command is not immediately visible.
'@
}

for ($Index = 0; $Index -lt $args.Count; $Index++) {
  switch ($args[$Index]) {
    { $_ -in '--help', '-h' } { Show-Usage; exit 0 }
    '--install-deps' { $InstallDeps = $true }
    '--bundles' {
      $Index++
      if ($Index -ge $args.Count -or [string]::IsNullOrWhiteSpace($args[$Index])) {
        throw '--bundles requires a value.'
      }
      $Bundles = $args[$Index]
    }
    '--jobs' {
      $Index++
      if ($Index -ge $args.Count -or [string]::IsNullOrWhiteSpace($args[$Index])) {
        throw '--jobs requires a value.'
      }
      $Jobs = $args[$Index]
    }
    { $_ -in '--unsigned', '--no-prompt' } {
      if ($SigningMode -ne 'default') { throw 'Choose only one signing option.' }
      $SigningMode = $_
    }
    { $_ -in '--sign', '--notarize', '--notarize-only', '--unlock-keychain' } {
      throw "$_ is a macOS-only option. Windows signing is controlled by Tauri's signing environment variables."
    }
    default { throw "Unknown option: $($args[$Index]) (use --help)" }
  }
}
if ($Jobs -notmatch '^[1-9][0-9]*$') { throw '--jobs must be a positive integer.' }
if (-not $Bundles) { $Bundles = 'nsis,msi' }
if ($Bundles -notmatch '^(nsis|msi)(,(nsis|msi))*$') {
  throw 'Windows bundles must be nsis, msi, or nsis,msi.'
}
if (-not [Environment]::Is64BitOperatingSystem) { throw 'The Windows desktop build currently requires 64-bit Windows.' }
if ($Bundles -match '(^|,)msi(,|$)') {
  Write-Host 'MSI builds require Windows optional feature VBSCRIPT. If WiX light.exe fails, enable it under Settings > Apps > Optional features > More Windows features.'
}

function Test-Command([string]$Name) {
  return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}
function Refresh-ProcessPath {
  $Paths = @(
    [Environment]::GetEnvironmentVariable('Path', 'Machine'),
    [Environment]::GetEnvironmentVariable('Path', 'User'),
    $env:Path
  )
  $env:Path = (($Paths -join ';') -split ';' |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
    Select-Object -Unique) -join ';'
}
function Get-MsysRoot {
  foreach ($Candidate in @($env:MSYS2_ROOT, 'C:\msys64', 'C:\tools\msys64')) {
    if ($Candidate -and (Test-Path (Join-Path $Candidate 'usr\bin\bash.exe'))) { return $Candidate }
  }
  return $null
}
function Test-VisualStudioCpp {
  $VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (-not (Test-Path $VsWhere)) { return $false }
  $Install = & $VsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  return $LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace(($Install | Select-Object -First 1))
}
function Get-Python {
  if (Test-Command 'python3') { return 'python3' }
  if (Test-Command 'python') { return 'python' }
  if (Test-Command 'py') { return 'py' }
  return $null
}
function Test-MsysTools([string]$Root) {
  if (-not $Root) { return $false }
  $Bash = Join-Path $Root 'usr\bin\bash.exe'
  & $Bash -lc 'export PATH=/ucrt64/bin:/usr/bin; command -v gcc >/dev/null && command -v make >/dev/null && command -v curl >/dev/null && command -v tar >/dev/null && command -v xz >/dev/null && (command -v python3 >/dev/null || command -v python >/dev/null)'
  return $LASTEXITCODE -eq 0
}
function Get-MissingDependencies {
  $Missing = [System.Collections.Generic.List[string]]::new()
  foreach ($Command in @('cargo', 'rustc', 'node', 'npm', 'cmake')) {
    if (-not (Test-Command $Command)) { $Missing.Add($Command) }
  }
  if (-not (Get-Python)) { $Missing.Add('Python 3') }
  if (-not (Test-VisualStudioCpp)) { $Missing.Add('Visual Studio 2022 C++ Build Tools') }
  $Msys = Get-MsysRoot
  if (-not $Msys) { $Missing.Add('MSYS2 UCRT64') }
  elseif (-not (Test-MsysTools $Msys)) { $Missing.Add('MSYS2 UCRT64 GCC/make tools') }
  return $Missing
}
function Show-DependencyHelp([string[]]$Missing) {
  Write-Error -ErrorAction Continue "Missing Windows build prerequisites: $($Missing -join ', ')"
  @'
Install from an Administrator PowerShell if needed:
  winget install --id Rustlang.Rustup --exact --accept-package-agreements --accept-source-agreements
  winget install --id OpenJS.NodeJS.LTS --exact --accept-package-agreements --accept-source-agreements
  winget install --id Python.Python.3.13 --exact --accept-package-agreements --accept-source-agreements
  winget install --id Kitware.CMake --exact --accept-package-agreements --accept-source-agreements
  winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" --accept-package-agreements --accept-source-agreements
  winget install --id MSYS2.MSYS2 --exact --accept-package-agreements --accept-source-agreements
Then install the FFmpeg toolchain:
  C:\msys64\usr\bin\bash.exe -lc "pacman -Sy --needed --noconfirm make diffutils curl tar xz python mingw-w64-ucrt-x86_64-gcc"
Reopen PowerShell, then rerun .\build.ps1. Rust must use the x86_64-pc-windows-msvc host.
'@ | Write-Host
}
function Confirm-DependencyInstall {
  if ($InstallDeps) { return $true }
  if (-not [Environment]::UserInteractive -or [Console]::IsInputRedirected) { return $false }
  $Answer = Read-Host 'Install the missing build dependencies now? [y/N]'
  return $Answer -match '^(y|yes)$'
}
function Install-WingetPackage([string]$Id, [string]$Override = '') {
  $Arguments = @('install', '--id', $Id, '--exact', '--accept-package-agreements', '--accept-source-agreements')
  if ($Override) { $Arguments += @('--override', $Override) }
  & winget @Arguments
  if ($LASTEXITCODE -ne 0) { throw "winget could not install $Id (exit $LASTEXITCODE)." }
}
function Install-Dependencies([string[]]$Missing) {
  if (-not (Test-Command 'winget')) {
    throw 'winget is unavailable. Install or update App Installer from Microsoft Store, then use the commands shown above.'
  }
  if ($Missing -contains 'cargo' -or $Missing -contains 'rustc') { Install-WingetPackage 'Rustlang.Rustup' }
  if ($Missing -contains 'node' -or $Missing -contains 'npm') { Install-WingetPackage 'OpenJS.NodeJS.LTS' }
  if ($Missing -contains 'Python 3') { Install-WingetPackage 'Python.Python.3.13' }
  if ($Missing -contains 'cmake') { Install-WingetPackage 'Kitware.CMake' }
  if ($Missing -contains 'Visual Studio 2022 C++ Build Tools') {
    Install-WingetPackage 'Microsoft.VisualStudio.2022.BuildTools' '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
  }
  if ($Missing -contains 'MSYS2 UCRT64') { Install-WingetPackage 'MSYS2.MSYS2' }
  $Msys = Get-MsysRoot
  if ($Msys -and (($Missing -contains 'MSYS2 UCRT64') -or ($Missing -contains 'MSYS2 UCRT64 GCC/make tools'))) {
    $Bash = Join-Path $Msys 'usr\bin\bash.exe'
    & $Bash -lc 'pacman -Sy --needed --noconfirm make diffutils curl tar xz python mingw-w64-ucrt-x86_64-gcc'
    if ($LASTEXITCODE -ne 0) { throw 'MSYS2 could not install the UCRT64 FFmpeg build tools.' }
  }
}
function Assert-Success([string]$Step) {
  if ($LASTEXITCODE -ne 0) { throw "$Step failed (exit $LASTEXITCODE)." }
}

$Missing = @(Get-MissingDependencies)
if ($Missing.Count -gt 0) {
  Show-DependencyHelp $Missing
  if (-not (Confirm-DependencyInstall)) { exit 1 }
  Install-Dependencies $Missing
  Refresh-ProcessPath
  $Missing = @(Get-MissingDependencies)
  if ($Missing.Count -gt 0) {
    Show-DependencyHelp $Missing
    throw 'Some newly installed tools are not visible yet. Reopen PowerShell and rerun the build.'
  }
}

$NodeVersion = (& node -p 'process.versions.node').Trim()
Assert-Success 'Node.js version check'
$NodeParts = $NodeVersion.Split('.')
if ([int]$NodeParts[0] -lt 22 -or ([int]$NodeParts[0] -eq 22 -and [int]$NodeParts[1] -lt 12)) {
  Write-Host "Node.js 22.12 or newer is required; found v$NodeVersion."
  if (-not (Confirm-DependencyInstall)) { throw 'Run winget upgrade --id OpenJS.NodeJS.LTS --exact, then reopen PowerShell.' }
  if (-not (Test-Command 'winget')) { throw 'winget is unavailable. Install or update App Installer from Microsoft Store, then upgrade Node.js.' }
  & winget upgrade --id OpenJS.NodeJS.LTS --exact --accept-package-agreements --accept-source-agreements
  Assert-Success 'Node.js upgrade'
  throw 'Node.js was upgraded. Reopen PowerShell so the new version is active, then rerun the build.'
}
$RustLine = (& rustc --version).Trim()
Assert-Success 'Rust version check'
$RustVersion = [version](($RustLine -split ' ')[1])
if ($RustVersion -lt [version]'1.88.0') {
  Write-Host "Rust 1.88 or newer is required; found $RustLine."
  if (-not (Confirm-DependencyInstall) -or -not (Test-Command 'rustup')) { throw 'Run rustup update stable.' }
  & rustup update stable
  Assert-Success 'Rust update'
  throw 'Rust was updated. Rerun the build with the stable toolchain active.'
}
$Target = ((& rustc -vV) | Where-Object { $_ -like 'host: *' } | Select-Object -First 1).Substring(6).Trim()
if ($Target -ne 'x86_64-pc-windows-msvc') {
  throw "The Windows build currently requires the x86_64-pc-windows-msvc Rust host; found $Target."
}

$Python = Get-Python
$Stage = Join-Path $ProjectRoot 'desktop\src-tauri'
$CacheRelative = "desktop/.build/$Target"
$Cache = Join-Path $ProjectRoot "desktop\.build\$Target"
$env:CARGO_BUILD_TARGET = $null
$env:CARGO_TARGET_DIR = Join-Path $ProjectRoot 'target'

Push-Location $ProjectRoot
try {
  & npm ci --prefix web
  Assert-Success 'Frontend dependency installation'
  & npm run build --prefix web
  Assert-Success 'Frontend build'
  & cargo build --release --locked -p pr0-server -j $Jobs
  Assert-Success 'Server build'

  Write-Host 'Preparing bundled FFmpeg (the first compilation can take several minutes)...'
  $Msys = Get-MsysRoot
  $Bash = Join-Path $Msys 'usr\bin\bash.exe'
  $env:CHERE_INVOKING = '1'
  & $Bash -lc "export MSYSTEM=UCRT64; export PATH=/ucrt64/bin:/usr/bin; bash scripts/build-ffmpeg.sh '$CacheRelative' '$Jobs'"
  Assert-Success 'FFmpeg build'

  $BinaryDirectory = Join-Path $Stage 'binaries'
  $WebResources = Join-Path $Stage 'resources\web'
  $FfmpegLicenses = Join-Path $Stage 'resources\licenses\ffmpeg'
  New-Item -ItemType Directory -Force $BinaryDirectory, $FfmpegLicenses | Out-Null
  if (Test-Path $WebResources) { Remove-Item -Recurse -Force $WebResources }
  Copy-Item -Recurse (Join-Path $ProjectRoot 'web\dist') $WebResources
  Copy-Item (Join-Path $ProjectRoot 'target\release\pr0-server.exe') (Join-Path $BinaryDirectory "pr0-server-$Target.exe") -Force
  Copy-Item (Join-Path $Cache 'ffmpeg-8.1.1\ffmpeg.exe') (Join-Path $BinaryDirectory "ffmpeg-$Target.exe") -Force
  Copy-Item (Join-Path $Cache 'ffmpeg-8.1.1.tar.xz') $FfmpegLicenses -Force
  Copy-Item (Join-Path $Cache 'ffmpeg-8.1.1\COPYING.LGPLv2.1') $FfmpegLicenses -Force
  Copy-Item (Join-Path $ProjectRoot 'scripts\build-ffmpeg.sh') (Join-Path $FfmpegLicenses 'build.sh') -Force
  Copy-Item (Join-Path $ProjectRoot 'LICENSE') (Join-Path $Stage 'resources\licenses\pr0former-MIT.txt') -Force
  Copy-Item (Join-Path $ProjectRoot 'desktop\THIRD_PARTY.md') (Join-Path $Stage 'resources\licenses') -Force
  if ($Python -eq 'py') {
    & $Python -3 (Join-Path $ProjectRoot 'scripts\collect-desktop-licenses.py') $ProjectRoot
  } else {
    & $Python (Join-Path $ProjectRoot 'scripts\collect-desktop-licenses.py') $ProjectRoot
  }
  Assert-Success 'Desktop license collection'

  & npm ci --prefix desktop
  Assert-Success 'Desktop dependency installation'
  Push-Location (Join-Path $ProjectRoot 'desktop')
  try {
    $env:CARGO_TARGET_DIR = Join-Path $Stage 'target'
    & (Join-Path $ProjectRoot 'desktop\node_modules\.bin\tauri.cmd') build --bundles $Bundles -- --locked -j $Jobs
    Assert-Success 'Tauri packaging'
  } finally {
    Pop-Location
  }
  Write-Host "Desktop bundles: $Stage\target\release\bundle\"
} finally {
  Pop-Location
}

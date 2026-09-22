$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
Set-Location $repoRoot

function Fail($message) {
  [Console]::Error.WriteLine("test-install.ps1: FAIL: $message")
  if ($script:LastLog -and (Test-Path $script:LastLog)) {
    [Console]::Error.WriteLine("--- the installer said ---")
    foreach ($line in Get-Content $script:LastLog) {
      [Console]::Error.WriteLine($line)
    }
    [Console]::Error.WriteLine("--------------------------")
  }
  exit 1
}

function Pass($message) {
  Write-Host "  ok  $message"
}

function CommandPath($name) {
  $command = Get-Command $name -ErrorAction SilentlyContinue
  if (-not $command) {
    return $null
  }
  return $command.Source
}

$installer = Join-Path $repoRoot "infra/cloudflare/setup-assets/install.ps1"
$releaseDir = $env:UF_TEST_RELEASE_DIR
if (-not $releaseDir) {
  Fail "UF_TEST_RELEASE_DIR is required for the Windows installer test"
}
$releaseDir = Resolve-Path $releaseDir
$version = (Get-Content (Join-Path $releaseDir "VERSION") -Raw).Trim()
if (-not $version) {
  Fail "VERSION in $releaseDir is empty"
}

switch -Regex ($env:PROCESSOR_ARCHITECTURE) {
  "^(AMD64|x86_64)$" { $target = "x86_64-pc-windows-msvc" }
  default { Fail "unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}

$archive = "uf-$target.tar.gz"
if (-not (Test-Path (Join-Path $releaseDir $archive))) {
  Fail "missing $archive in $releaseDir"
}

$python = CommandPath "python"
if (-not $python) {
  $python = CommandPath "python3"
}
if (-not $python) {
  Fail "python or python3 is required"
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("uf-test-install-" + [System.Guid]::NewGuid().ToString("N"))
$site = Join-Path $work "site/uf"
$versionDir = Join-Path $site $version
$latestDir = Join-Path $site "latest"
New-Item -ItemType Directory -Path $versionDir, $latestDir | Out-Null
Copy-Item (Join-Path $releaseDir $archive), (Join-Path $releaseDir "$archive.sha256"), (Join-Path $releaseDir "VERSION") $versionDir
Copy-Item (Join-Path $versionDir "*") $latestDir

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()

$serverOut = Join-Path $work "server.out"
$serverErr = Join-Path $work "server.err"
$server = Start-Process -FilePath $python `
  -ArgumentList @("-m", "http.server", "$port", "--bind", "127.0.0.1", "--directory", (Join-Path $work "site")) `
  -RedirectStandardOutput $serverOut `
  -RedirectStandardError $serverErr `
  -PassThru

try {
  $ready = $false
  for ($i = 0; $i -lt 100; $i++) {
    try {
      Invoke-WebRequest -Uri "http://127.0.0.1:$port/uf/latest/VERSION" -UseBasicParsing | Out-Null
      $ready = $true
      break
    } catch {
      Start-Sleep -Milliseconds 50
    }
  }
  if (-not $ready) {
    Fail "fixture server never came up"
  }

  $base = "http://127.0.0.1:$port/uf"

  function RunInstaller($caseName, $versionValue = $null) {
    $caseRoot = Join-Path $work $caseName
    $log = Join-Path $work "$caseName.log"
    $script:LastLog = $log
    $oldReleaseBase = $env:UF_RELEASE_BASE
    $oldInstallRoot = $env:UF_INSTALL_ROOT
    $oldBinDir = $env:UF_BIN_DIR
    $oldVersion = $env:UF_VERSION
    try {
      $env:UF_RELEASE_BASE = $base
      $env:UF_INSTALL_ROOT = Join-Path $caseRoot "share"
      $env:UF_BIN_DIR = Join-Path $caseRoot "bin"
      if ($null -eq $versionValue) {
        Remove-Item Env:UF_VERSION -ErrorAction SilentlyContinue
      } else {
        $env:UF_VERSION = $versionValue
      }
      & pwsh -NoProfile -ExecutionPolicy Bypass -File $installer *>&1 | Set-Content $log
      return $LASTEXITCODE -eq 0
    } finally {
      if ($null -eq $oldReleaseBase) { Remove-Item Env:UF_RELEASE_BASE -ErrorAction SilentlyContinue } else { $env:UF_RELEASE_BASE = $oldReleaseBase }
      if ($null -eq $oldInstallRoot) { Remove-Item Env:UF_INSTALL_ROOT -ErrorAction SilentlyContinue } else { $env:UF_INSTALL_ROOT = $oldInstallRoot }
      if ($null -eq $oldBinDir) { Remove-Item Env:UF_BIN_DIR -ErrorAction SilentlyContinue } else { $env:UF_BIN_DIR = $oldBinDir }
      if ($null -eq $oldVersion) { Remove-Item Env:UF_VERSION -ErrorAction SilentlyContinue } else { $env:UF_VERSION = $oldVersion }
    }
  }

  Write-Host "test-install.ps1: $version ($target) via $base"

  if (-not (RunInstaller "latest")) {
    Fail "installing latest failed"
  }
  foreach ($name in @("uf", "ufr", "ufx")) {
    if (-not (Test-Path (Join-Path $work "latest/bin/$name.cmd"))) {
      Fail "latest: $name.exe was not installed"
    }
  }
  $installedVersion = & (Join-Path $work "latest/bin/uf.cmd") --version
  if ($installedVersion -notmatch [regex]::Escape($version)) {
    Fail "latest: uf --version said '$installedVersion', expected $version"
  }
  Pass "latest resolves, installs uf/ufr/ufx, and runs"

  if (-not (RunInstaller "pinned" "uf@$version")) {
    Fail "installing uf@$version failed"
  }
  if (-not (Test-Path (Join-Path $work "pinned/bin/uf.cmd"))) {
    Fail "pinned: uf.exe was not installed"
  }
  Pass "UF_VERSION=uf@$version installs the pinned release"

  # Execute the installed runtime while it updates the launchers it was reached
  # through. Replacing an active .exe in place fails on Windows.
  $env:UF_INSTALL_ROOT = Join-Path $work "pinned/share"
  $env:UF_BIN_DIR = Join-Path $work "pinned/bin"
  $env:XDG_STATE_HOME = Join-Path $work "state"
  $env:UF_RELEASE_BASE = $base
  & (Join-Path $work "pinned/bin/uf.cmd") self-update $version
  if ($LASTEXITCODE -ne 0) { Fail "self-update through installed uf failed" }
  & (Join-Path $work "pinned/bin/uf.cmd") --version
  if ($LASTEXITCODE -ne 0) { Fail "updated launcher failed" }
  & (Join-Path $work "pinned/bin/uf.cmd") self-update
  if ($LASTEXITCODE -ne 0) { Fail "self-update latest through the embedded installer failed" }
  Pass "installed uf updates pinned and latest without replacing its running executable"

  foreach ($bad in @("../escape", "*", ".hidden", "-flag")) {
    if (RunInstaller "invalid" $bad) { Fail "accepted invalid version $bad" }
  }
  Pass "versions cannot escape the runtime directory"


  $tamperedDir = Join-Path $site "tampered"
  New-Item -ItemType Directory -Path $tamperedDir | Out-Null
  Copy-Item (Join-Path $versionDir "$archive.sha256"), (Join-Path $versionDir "VERSION") $tamperedDir
  Set-Content -Path (Join-Path $tamperedDir $archive) -Value "not an archive" -NoNewline
  if (RunInstaller "tampered" "tampered") {
    Fail "a tampered archive was installed"
  }
  $tamperedLog = Get-Content (Join-Path $work "tampered.log") -Raw
  if ($tamperedLog -notmatch "checksum mismatch") {
    Fail "tampered archive was rejected, but not for the checksum"
  }
  Pass "a tampered archive fails the checksum and installs nothing"
} finally {
  if ($server -and -not $server.HasExited) {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
  }
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

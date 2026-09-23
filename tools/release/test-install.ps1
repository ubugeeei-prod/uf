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
  # And what the fixture server was asked, and what it answered. A request it
  # answered 200 that the installer still refused is the installer's bug; one
  # it never received is the fixture's. alpha.44's release check failed here
  # with "no version at .../latest/VERSION" for a file this server had served,
  # and nothing on the page could tell the two apart (#1328).
  if ($script:server) {
    if (-not $script:server.HasExited) {
      Stop-Process -Id $script:server.Id -Force -ErrorAction SilentlyContinue
      $script:server.WaitForExit()
    }
    if ($script:serverErr -and (Test-Path $script:serverErr)) {
      [Console]::Error.WriteLine("--- the fixture server saw ---")
      foreach ($line in Get-Content $script:serverErr -Tail 40) {
        [Console]::Error.WriteLine($line)
      }
      [Console]::Error.WriteLine("------------------------------")
    }
  }
  exit 1
}

# VERSION decoded the way install.ps1's ReadVersion does: a generic file
# server sends it as application/octet-stream, which PowerShell hands back as
# byte[] rather than a string.
function ServedText($url) {
  $content = (Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 5).Content
  if ($content -is [byte[]]) {
    $content = [System.Text.Encoding]::UTF8.GetString($content)
  }
  return ([string]$content).Trim()
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

$server = $null
$serverErr = $null
$base = $null

try {
  # A free port is chosen, released, and handed to Python, and something else
  # can take it in between. That shows as the server exiting, or as a server
  # that does not answer with this release's VERSION; either is retried on a
  # fresh port instead of being reported as an installer failure. Readiness is
  # the content, not just an answer: the installer is only run against a
  # mirror that has been seen serving what it will be asked for.
  for ($attempt = 1; $attempt -le 3 -and -not $base; $attempt++) {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()

    $serverErr = Join-Path $work "server-$attempt.err"
    $server = Start-Process -FilePath $python `
      -ArgumentList @("-m", "http.server", "$port", "--bind", "127.0.0.1", "--directory", (Join-Path $work "site")) `
      -RedirectStandardOutput (Join-Path $work "server-$attempt.out") `
      -RedirectStandardError $serverErr `
      -PassThru

    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while ([DateTime]::UtcNow -lt $deadline -and -not $server.HasExited) {
      try {
        $served = ServedText "http://127.0.0.1:$port/uf/latest/VERSION"
      } catch {
        Start-Sleep -Milliseconds 50
        continue
      }
      if ($served -eq $version) {
        $base = "http://127.0.0.1:$port/uf"
      } else {
        Write-Host "test-install.ps1: port $port answered '$served' for latest/VERSION, not $version; retrying"
      }
      break
    }
    if (-not $base -and -not $server.HasExited) {
      Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
      $server.WaitForExit()
    }
  }
  if (-not $base) {
    Fail "the fixture server never served latest/VERSION as $version"
  }

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
    $launcher = Join-Path $work "latest/bin/$name.cmd"
    if (-not (Test-Path $launcher)) {
      Fail "latest: $name.cmd was not installed"
    }
    $installedVersion = & $launcher --version
    if ($LASTEXITCODE -ne 0 -or $installedVersion -notmatch [regex]::Escape($version)) {
      Fail "latest: $name --version failed or reported '$installedVersion', expected $version"
    }
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

$ErrorActionPreference = "Stop"

$repo = if ($env:UF_REPO) { $env:UF_REPO } else { "ubugeeei-prod/uf" }
$releaseBase = $env:UF_RELEASE_BASE
$requestedVersion = if ($env:UF_VERSION) { $env:UF_VERSION } else { "latest" }
$verifyOrigin = if ($env:UF_VERIFY_ORIGIN) { $env:UF_VERIFY_ORIGIN } else { "auto" }
$checksumBase = $env:UF_CHECKSUM_BASE
$signingWorkflow = if ($env:UF_SIGNING_WORKFLOW) { $env:UF_SIGNING_WORKFLOW } else { ".github/workflows/release.yml" }
$signingIssuer = if ($env:UF_SIGNING_ISSUER) { $env:UF_SIGNING_ISSUER } else { "https://token.actions.githubusercontent.com" }
$installRoot = if ($env:UF_INSTALL_ROOT) {
  $env:UF_INSTALL_ROOT
} elseif ($env:LOCALAPPDATA) {
  Join-Path $env:LOCALAPPDATA "uf"
} else {
  Join-Path $HOME ".local/share/uf"
}
$binDir = if ($env:UF_BIN_DIR) {
  $env:UF_BIN_DIR
} else {
  Join-Path $HOME ".local/bin"
}
$stopAfter = if ($env:UF_STOP_AFTER) { $env:UF_STOP_AFTER } else { "" }

function Fail($message, $hint = $null) {
  [Console]::Error.WriteLine("")
  [Console]::Error.WriteLine("  x $message")
  if ($hint) {
    [Console]::Error.WriteLine("    $hint")
  }
  exit 1
}

function Step($verb, $message) {
  Write-Host "  + $verb $message"
}

function Note($message) {
  Write-Host "  $message"
}

function Fetch($url, $path, $message) {
  try {
    Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
  } catch {
    Fail $message "$url ($($_.Exception.Message))"
  }
}

function FetchOptional($url, $path) {
  try {
    Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
    return $true
  } catch {
    return $false
  }
}

function HostOf($url) {
  try {
    $uri = [System.Uri]$url
    return "$($uri.Scheme)://$($uri.Authority)"
  } catch {
    return ""
  }
}

function VerifyOrigin($archive, $archivePath, $archiveUrl, $signatureUrl, $expected, $tempRoot) {
  if ($verifyOrigin -eq "off") {
    Note "origin checking is off (UF_VERIFY_ORIGIN=off)"
    return
  }

  $origin = ""
  $signaturePath = Join-Path $tempRoot "$archive.sigstore"
  if (FetchOptional $signatureUrl $signaturePath) {
    $cosign = Get-Command cosign -ErrorAction SilentlyContinue
    if ($cosign) {
      $identity = "^https://github\.com/$([regex]::Escape($repo))/$([regex]::Escape($signingWorkflow))@"
      $cosignLog = Join-Path $tempRoot "cosign.log"
      & $cosign.Source verify-blob `
        --bundle $signaturePath `
        --certificate-identity-regexp $identity `
        --certificate-oidc-issuer $signingIssuer `
        $archivePath *> $cosignLog
      if ($LASTEXITCODE -eq 0) {
        $origin = "signature"
        Step "verified" "signed by $repo $signingWorkflow"
      } else {
        $cosignTail = ""
        if (Test-Path $cosignLog) {
          $cosignTail = (Get-Content $cosignLog -Tail 3 -ErrorAction SilentlyContinue) -join " "
        }
        Fail "cosign would not verify the signature on $archive" "$cosignTail either it was not signed by $repo $signingWorkflow, or this cosign cannot read the bundle - do not run this binary"
      }
    } else {
      Note "this release is signed and cosign is not installed, so the signature was not checked"
    }
  } else {
    Note "this release publishes no signature"
  }

  if ($checksumBase) {
    $secondUrl = "$checksumBase/$version/$archive.sha256"
    if ((HostOf $secondUrl) -eq (HostOf $archiveUrl)) {
      Note "UF_CHECKSUM_BASE is on the same host as the archive, so it is not a second opinion"
    } else {
      $secondPath = Join-Path $tempRoot "second.sha256"
      if (FetchOptional $secondUrl $secondPath) {
        $second = ((Get-Content $secondPath -Raw) -split "\s+")[0].ToLowerInvariant()
        if ($second -ne $expected) {
          Fail "two hosts disagree about $archive" "$(HostOf $archiveUrl) says $expected; $(HostOf $secondUrl) says $second - do not run this binary"
        }
        if (-not $origin) {
          $origin = "second opinion"
        }
        Step "verified" "$(HostOf $secondUrl) agrees"
      } else {
        Note "no second-opinion checksum at $secondUrl"
      }
    }
  }

  if ($origin) {
    return
  }
  if ($verifyOrigin -eq "require") {
    Fail "the origin of $archive could not be established" "UF_VERIFY_ORIGIN=require needs a Sigstore signature (install cosign) or UF_CHECKSUM_BASE"
  }
  Note "origin not proven: the checksum came from the host that served the archive"
}

function LatestPrereleaseTag() {
  $headers = @{ Accept = "application/vnd.github+json" }
  $token = if ($env:GITHUB_TOKEN) { $env:GITHUB_TOKEN } else { $env:GH_TOKEN }
  if ($token) {
    $headers.Authorization = "Bearer $token"
  }
  try {
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases?per_page=1" -Headers $headers
    if ($releases.Count -gt 0) {
      return $releases[0].tag_name
    }
  } catch {
    return ""
  }
  return ""
}

function GetTarget() {
  switch -Regex ($env:PROCESSOR_ARCHITECTURE) {
    "^(AMD64|x86_64)$" { return "x86_64-pc-windows-msvc" }
    default { Fail "no build for $env:PROCESSOR_ARCHITECTURE" "uf ships x86_64 Windows binaries today" }
  }
}

switch ($stopAfter) {
  "" {}
  "resolve" {}
  "unpack" {}
  default { Fail "UF_STOP_AFTER=$stopAfter is not one uf understands" "it is resolve, unpack, or unset to install and link" }
}
switch ($verifyOrigin) {
  "auto" {}
  "require" {}
  "off" {}
  default { Fail "UF_VERIFY_ORIGIN=$verifyOrigin is not one uf understands" "it is auto, require, or off" }
}

if ($requestedVersion.StartsWith("uf@")) {
  $requestedVersion = $requestedVersion.Substring(3)
}

$target = GetTarget
$version = $requestedVersion

if ($releaseBase) {
  $channelUrl = "$releaseBase/$requestedVersion"
  if ($requestedVersion -eq "latest") {
    try {
      $version = (Invoke-WebRequest -Uri "$channelUrl/VERSION" -UseBasicParsing).Content.Trim()
    } catch {
      Fail "no version at $channelUrl/VERSION" "set UF_VERSION to install a specific release"
    }
  }
} elseif ($requestedVersion -eq "latest") {
  $stableUrl = "https://github.com/$repo/releases/latest/download"
  try {
    $version = (Invoke-WebRequest -Uri "$stableUrl/VERSION" -UseBasicParsing).Content.Trim()
    $channelUrl = $stableUrl
  } catch {
    $tag = LatestPrereleaseTag
    $version = $tag -replace "^uf@", ""
    if (-not $version) {
      Fail "no release found for $repo" "set GITHUB_TOKEN, or set UF_VERSION to install a specific release"
    }
    $channelUrl = "https://github.com/$repo/releases/download/uf@$version"
  }
} else {
  $channelUrl = "https://github.com/$repo/releases/download/uf@$requestedVersion"
}

if ($stopAfter -eq "resolve") {
  Write-Output $version
  exit 0
}

Write-Host ""
Write-Host "Unified Toolchain for Flow"
Write-Host ""
Write-Host ("  {0,-9}{1}" -f "target", $target)
Write-Host ("  {0,-9}{1}" -f "version", $version)
Write-Host ""

$archive = "uf-$target.tar.gz"
$archiveUrl = "$channelUrl/$archive"
$checksumUrl = "$archiveUrl.sha256"
$signatureUrl = "$archiveUrl.sigstore"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("uf-install-" + [System.Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $tempRoot $archive
$checksumPath = "$archivePath.sha256"
$stagingDir = Join-Path $tempRoot "stage"

New-Item -ItemType Directory -Path $tempRoot, $stagingDir | Out-Null
try {
  Fetch $archiveUrl $archivePath "could not download $archive"
  Fetch $checksumUrl $checksumPath "could not download the checksum for $archive"
  Step "downloaded" $archive

  $expected = ((Get-Content $checksumPath -Raw) -split "\s+")[0].ToLowerInvariant()
  $actual = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
  if ($actual -ne $expected) {
    Fail "checksum mismatch for $archive" "expected $expected, got $actual"
  }
  Step "verified" ("sha256 " + $expected.Substring(0, 12))

  VerifyOrigin $archive $archivePath $archiveUrl $signatureUrl $expected $tempRoot

  $members = & tar -tzf $archivePath
  if ($LASTEXITCODE -ne 0) {
    Fail "$archive could not be listed" "the archive is corrupt"
  }
  foreach ($member in $members) {
    $normalized = $member -replace "\\", "/"
    if ($normalized.StartsWith("/") -or $normalized -match "^[A-Za-z]:" -or ($normalized -split "/") -contains "..") {
      Fail "$archive writes outside its own directory" "the archive is not one uf published - do not unpack it"
    }
  }

  & tar -xzf $archivePath -C $stagingDir
  if ($LASTEXITCODE -ne 0) {
    Fail "$archive could not be unpacked"
  }
  foreach ($name in @("uf", "ufr", "ufx")) {
    $exe = Join-Path $stagingDir "bin/$name.exe"
    if (-not (Test-Path $exe -PathType Leaf)) {
      Fail "the archive has no bin/$name.exe" "the release is incomplete"
    }
  }

  $runtimesDir = Join-Path $installRoot "runtimes"
  $runtimeDir = Join-Path $runtimesDir "uf@$version"
  $incomingDir = Join-Path $runtimesDir (".uf@$version." + [System.Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $runtimesDir, $binDir -Force | Out-Null
  Move-Item $stagingDir $incomingDir

  if (Test-Path $runtimeDir) {
    Remove-Item -Recurse -Force $runtimeDir
  }
  Move-Item $incomingDir $runtimeDir
  Step "installed" $runtimeDir

  if ($stopAfter -eq "unpack") {
    exit 0
  }

  foreach ($name in @("uf", "ufr", "ufx")) {
    Copy-Item -Force (Join-Path $runtimeDir "bin/$name.exe") (Join-Path $binDir "$name.exe")
  }
  Step "linked" $binDir

  if (-not (($env:PATH -split ";") -contains $binDir)) {
    Write-Host "  add $binDir to PATH to use uf from a new terminal"
  }
} finally {
  Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
}

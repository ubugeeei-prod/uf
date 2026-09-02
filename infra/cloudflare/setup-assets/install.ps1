$ErrorActionPreference = "Stop"

$releaseBase = if ($env:UF_RELEASE_BASE) { $env:UF_RELEASE_BASE } else { "https://releases.uniflowed.dev/uf" }
$requestedVersion = if ($env:UF_VERSION) { $env:UF_VERSION } else { "latest" }

if ($requestedVersion.StartsWith("uf@")) {
  $requestedVersion = $requestedVersion.Substring(3)
}

$channelUrl = "$releaseBase/$requestedVersion"
$version = $requestedVersion
if ($requestedVersion -eq "latest") {
  $version = (Invoke-WebRequest -UseBasicParsing "$channelUrl/VERSION").Content.Trim()
}

Write-Error "uf Windows installer is reserved, but Windows artifacts are not published yet. Requested uf@$version."

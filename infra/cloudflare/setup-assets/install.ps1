$ErrorActionPreference = "Stop"

# Reserved so the URL resolves and says something useful. The release workflow
# publishes macOS and Linux only, so resolving a version here would just 404
# before reaching this message.
$requestedVersion = if ($env:UF_VERSION) { $env:UF_VERSION } else { "latest" }
if ($requestedVersion.StartsWith("uf@")) {
  $requestedVersion = $requestedVersion.Substring(3)
}

Write-Error @"
uf does not publish Windows artifacts yet (requested uf@$requestedVersion).

Supported today: macOS and Linux, on x86_64 and aarch64, via
  curl -fsSL https://setup.uniflowed.dev | sh

On Windows, run uf under WSL2, or build from source:
  cargo install --git https://github.com/ubugeeei-prod/uf uf_cli
"@

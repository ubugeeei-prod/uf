$ErrorActionPreference = "Stop"
$installer = (Resolve-Path (Join-Path $PSScriptRoot "../../infra/cloudflare/setup-assets/install.ps1")).Path
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("uf-version-response-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory $work | Out-Null
$serverSource = @'
const http = require("node:http");
const fs = require("node:fs");
const server = http.createServer((request, response) => {
  if (request.url.startsWith("/missing/")) {
    response.writeHead(404, { "Content-Type": "text/plain" });
    response.end("not found");
    return;
  }
  response.writeHead(200, { "Content-Type": request.url.startsWith("/text/") ? "text/plain; charset=utf-8" : "application/octet-stream" });
  response.end(Buffer.from("0.1.0-alpha.1\r\n", "utf8"));
});
// Written aside and renamed: a reader polling for `port` must never see it
// created and still empty.
server.listen(0, "127.0.0.1", () => {
  fs.writeFileSync("port.partial", String(server.address().port));
  fs.renameSync("port.partial", "port");
});
'@
Set-Content -Path (Join-Path $work "server.cjs") -Value $serverSource -Encoding utf8
$server = Start-Process -FilePath (Get-Command node).Source -ArgumentList "server.cjs" -WorkingDirectory $work -PassThru
try {
  $portFile = Join-Path $work "port"
  for ($attempt = 0; $attempt -lt 100 -and -not (Test-Path $portFile); $attempt++) {
    if ($server.HasExited) { throw "HTTP fixture exited before listening" }
    Start-Sleep -Milliseconds 50
  }
  $port = (Get-Content $portFile -Raw).Trim()
  $env:UF_STOP_AFTER = "resolve"
  Remove-Item Env:UF_VERSION -ErrorAction SilentlyContinue
  foreach ($content in @("binary", "text")) {
    $env:UF_RELEASE_BASE = "http://127.0.0.1:$port/$content"
    $resolved = & pwsh -NoProfile -ExecutionPolicy Bypass -File $installer 2>&1
    if ($LASTEXITCODE -ne 0 -or ($resolved -join "").Trim() -ne "0.1.0-alpha.1") {
      throw "VERSION served as $content did not resolve: $resolved"
    }
    Write-Host "ok: $content VERSION resolves and trims the response"
  }
  # A VERSION that cannot be read says why. alpha.44's release check failed
  # with only "no version at <url>" for a file the server had answered 200
  # with, and that was read as a transient miss rather than a bug (#1328).
  $env:UF_RELEASE_BASE = "http://127.0.0.1:$port/missing"
  $refused = & pwsh -NoProfile -ExecutionPolicy Bypass -File $installer 2>&1
  if ($LASTEXITCODE -eq 0) {
    throw "a missing VERSION resolved: $refused"
  }
  if (($refused -join "`n") -notmatch "no version at .*/missing/latest/VERSION \(.*404") {
    throw "a missing VERSION failed without saying why: $refused"
  }
  Write-Host "ok: an unreadable VERSION fails with the reason it could not be read"
} finally {
  if (-not $server.HasExited) { Stop-Process -Id $server.Id -Force }
  Remove-Item -Recurse -Force $work
}

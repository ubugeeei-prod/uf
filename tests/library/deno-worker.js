// @flow
// Nested fixture workers need the same host as their parent. These grants are
// for the repository's process/filesystem tests, not an application's defaults.
export function denoWorkerArguments(repository: string): Array<string> {
  return [
    "run",
    "--allow-read",
    "--allow-write",
    "--allow-env",
    "--allow-run",
    "--allow-net=127.0.0.1,localhost,[::1]",
    "--allow-ffi",
    "--allow-sys=uid,homedir",
    "--preload",
    `${repository}/packages/host/deno-preload.js`,
  ];
}

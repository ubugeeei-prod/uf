// @flow
// Previously supported keys; keep this comment and custom tasks.
export default {
  env: { toolchain: { node: "24.14.0", npm: "11.9.0" } },
  builder: { module: "@uniflowed/vite" },
  pm: { packageManager: "npm" },
  test: { runner: { applicationTarget: "web" } },
  tasks: { custom: "echo still here" },
};

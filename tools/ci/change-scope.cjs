const { execFileSync } = require("node:child_process");
const { appendFileSync } = require("node:fs");

// Unknown paths and missing history run the full suite. Check both sides of
// renames so moving source into docs cannot hide a code change.
function needsFullSuite(paths) {
  return (
    paths.length === 0 ||
    paths.some(
      (path) =>
        !(
          /^(README|CONTRIBUTING|CHANGELOG|LICENSE)(\.[^/]*)?$/.test(path) ||
          path === "AGENTS.md" ||
          path.startsWith("docs/") ||
          path.startsWith("brand/")
        ),
    )
  );
}

if (require.main === module) {
  let full = true;
  try {
    const base = process.env.BASE_SHA;
    if (!/^[a-f0-9]{40}$/.test(base || "") || /^0+$/.test(base)) throw new Error("No base commit");
    const paths = execFileSync("git", ["diff", "--name-only", "--no-renames", "-z", base, "HEAD"], {
      encoding: "utf8",
    })
      .split("\0")
      .filter(Boolean);
    full = needsFullSuite(paths);
    console.log(`${paths.length} changed files; full suite: ${full}`);
  } catch (error) {
    console.log(`Full suite: ${error.message}`);
  }
  appendFileSync(process.env.GITHUB_OUTPUT, `full=${full}\n`);
}
module.exports = { needsFullSuite };

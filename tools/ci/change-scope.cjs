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
  let code = true;
  let release = false;
  let paths;
  const base = process.env.BASE_SHA;
  try {
    if (!/^[a-f0-9]{40}$/.test(base || "") || /^0+$/.test(base)) throw new Error("No base commit");
    paths = execFileSync("git", ["diff", "--name-only", "--no-renames", "-z", base, "HEAD"], {
      encoding: "utf8",
    })
      .split("\0")
      .filter(Boolean);
  } catch (error) {
    console.log(`Full suite: ${error.message}`);
  }
  if (paths) {
    // Authorization errors fail the job; they must never become a quick run.
    release = require("../release/policy.cjs").checkCandidate(base, paths);
    code = needsFullSuite(paths);
    full = release;
    console.log(`${paths.length} changed files; code: ${code}; full suite: ${full}`);
  }
  const version = release
    ? JSON.parse(require("node:fs").readFileSync("packages/core/package.json", "utf8")).version
    : "";
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `full=${full}\ncode=${code}\nrelease=${release}\nversion=${version}\n`,
  );
}
module.exports = { needsFullSuite };

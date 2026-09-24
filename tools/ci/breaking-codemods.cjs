const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
function check(root /*: string */, base /*: string */) /*: Array<string> */ {
  const git = (...args /*: Array<string> */) /*: string */ =>
    execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
  const catalog = JSON.parse(
    fs.readFileSync(path.join(root, "tools/codemods/catalog.json"), "utf8"),
  );
  const entries = JSON.parse(
    fs.readFileSync(path.join(root, "tools/codemods/breaking-changes.json"), "utf8"),
  );
  const changelog = fs.readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
  const commits = git("log", "--format=%H", base + "..HEAD")
    .split("\n")
    .filter(Boolean);
  const problems = [];
  for (const commit of commits) {
    const message = git("show", "-s", "--format=%B", commit);
    const subject = message.split("\n")[0].replace(/ \(#\d+\)$/, "");
    if (!/^[a-z]+(?:\([^)]*\))?!:/.test(subject) && !/^BREAKING[ -]CHANGE:/m.test(message))
      continue;
    const entry = entries.find((entry) => entry.change === subject);
    const migration = entry && catalog[entry.codemod];
    if (
      migration &&
      [migration.implementation, migration.fixture].every(
        (file) =>
          typeof file === "string" &&
          !path.isAbsolute(file) &&
          !file.split(/[\\/]/).includes("..") &&
          fs.existsSync(path.join(root, file)),
      )
    )
      continue;
    const prefix = "No codemod (" + subject + "): ";
    const explanation = changelog.split("\n").find((line) => line.includes(prefix));
    if (
      explanation &&
      explanation.slice(explanation.indexOf(prefix) + prefix.length).trim().length >= 12
    )
      continue;
    problems.push(
      subject +
        ': register its codemod in tools/codemods/breaking-changes.json or add "' +
        prefix +
        '<reason>" to CHANGELOG.md',
    );
  }
  return problems;
}
if (require.main === module) {
  const event = process.env.GITHUB_EVENT_PATH
    ? JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"))
    : {};
  const candidate = event.pull_request?.base?.sha ?? event.merge_group?.base_sha ?? event.before;
  const base =
    process.env.UF_BREAKING_BASE ??
    (candidate && !/^0+$/.test(candidate) ? candidate : "origin/main");
  const problems = check(process.cwd(), base);
  if (problems.length) {
    process.stderr.write(problems.join("\n") + "\n");
    process.exitCode = 1;
  } else process.stdout.write("Breaking changes have migration coverage.\n");
}
module.exports = { check };

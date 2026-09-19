// @noflow
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-native-stylex-types."));
try {
  fs.symlinkSync(
    path.resolve(process.argv[2], "node_modules"),
    path.join(root, "node_modules"),
    "dir",
  );
  fs.writeFileSync(
    path.join(root, "package.json"),
    '{"private":true,"type":"module","dependencies":{"@uniflowed/stylex":"*","react-native":"*"}}\n',
  );
  fs.writeFileSync(path.join(root, "uf.config.js"), "// @flow\nexport default {};\n");
  const valid = `// @flow
import { Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";
const styles = stylex.create({ box: { padding: 12 }, text: { color: "red", fontWeight: "700" } });
export component Sample(active: boolean) {
  return <View {...stylex.props(styles.box)}><Text {...stylex.props([styles.text, active && styles.text], null)}>Hello</Text></View>;
}
`;
  const invalid = `// @flow
import { stylex } from "@uniflowed/stylex/native";
const invalid = stylex.create({ bad: { fontWeight: "invalid-weight" } });
const styles = stylex.create({ label: { color: "red", fontWeight: "700" } });
const wrong: number = stylex.props(styles.label).style.color;
const browserClass: string = stylex.props(styles.label).className;
const missing = styles.missing;
import { Text } from "react-native";
export component Invalid() { return <Text {...stylex.props(invalid.bad)}>Bad weight</Text>; }
`;
  for (const [file, source] of [
    ["valid.js", valid],
    ["invalid.js", invalid],
  ]) {
    fs.writeFileSync(path.join(root, file), source);
    const child = spawnSync(process.env.UF_BINARY, ["--cwd", root, "check", file, "--json"], {
      encoding: "utf8",
      timeout: 180000,
      maxBuffer: 8 * 1024 * 1024,
    });
    if (child.error) throw child.error;
    const report = JSON.parse(child.stdout);
    assert.equal(report.typeCheck.status, "checked", "requires the real Flow checker");
    assert.equal(child.status, file === "valid.js" ? 0 : 1, child.stderr + child.stdout);
    if (file === "invalid.js") {
      const lines = new Set(
        report.typeCheck.diagnostics
          .filter((entry) => entry.primary.path === file)
          .map((entry) => entry.primary.start.line),
      );
      for (const line of [5, 6, 7, 9])
        assert.ok(lines.has(line), `missing native type error at line ${line}`);
    }
  }
  console.log(
    "Native StyleX accepts component props and rejects invalid values, web props and namespaces",
  );
} finally {
  fs.rmSync(root, { recursive: true });
}

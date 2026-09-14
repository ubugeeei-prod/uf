// @noflow
//
// The runtime behind project rules, without a process: what a rule sees, the
// order it sees it in, and what comes back. The process around it — budgets,
// restarts, the protocol — is tested from the other side, in
// `crates/uf_cli/tests/lint_plugins.rs`, against a real host.

import { describe, expect, it } from "@uniflowed/test";

import { interpolate, lineStartsOf, lintFile, mergeFixes, walk } from "./internal/lint-rules.js";

const SOURCE = "const foo = 1;";

/** `const foo = 1;` as uf's parser renders it, trimmed to what the runtime reads. */
function program() {
  const id = { type: "Identifier", name: "foo", range: [6, 9] };
  const init = { type: "Literal", value: 1, raw: "1", range: [12, 13] };
  // `init` before `id` on purpose: a tree that arrives through JSON has its
  // keys in whatever order the map kept, not in source order.
  const declarator = { type: "VariableDeclarator", init, id, range: [6, 13] };
  const declaration = { type: "VariableDeclaration", kind: "const", declarations: [declarator], range: [0, 14] };
  return { type: "Program", body: [declaration], comments: [], range: [0, 14] };
}

function run(rules, source = SOURCE, ast = program()) {
  return lintFile(
    { root: "/project", rules },
    { path: "app.js", filename: "/project/app.js", source, ast },
  );
}

describe("walk", () => {
  it("visits in source order, leaves after the children, and sets parent", () => {
    const ast = program();
    const seen = [];

    walk(ast, (key) => seen.push(key));

    expect(seen).toEqual([
      "Program",
      "VariableDeclaration",
      "VariableDeclarator",
      "Identifier",
      "Identifier:exit",
      "Literal",
      "Literal:exit",
      "VariableDeclarator:exit",
      "VariableDeclaration:exit",
      "Program:exit",
    ]);
    expect(ast.body[0].declarations[0].id.parent.type).toBe("VariableDeclarator");
  });
});

describe("lintFile", () => {
  it("reports through messageId and data, merging a rule's fixes into one edit", () => {
    const rule = {
      meta: { fixable: "code", messages: { reserved: "`{{ name }}` is reserved" } },
      create(context) {
        return {
          Identifier(node) {
            context.report({
              node,
              messageId: "reserved",
              data: { name: node.name },
              fix: (fixer) => [fixer.insertTextAfter(node, "Bar"), fixer.replaceText(node, "baz")],
            });
          },
        };
      },
    };

    const result = run([{ id: "acme/no-foo", rule }]);

    expect(result.diagnostics).toEqual([
      {
        rule: "acme/no-foo",
        message: "`foo` is reserved",
        start: 6,
        end: 9,
        fix: { start: 6, end: 9, text: "bazBar" },
      },
    ]);
    expect(result.problems).toEqual([]);
    expect(typeof result.micros["acme/no-foo"]).toBe("number");
  });

  it("names a rule that throws, and keeps the file's other rules running", () => {
    const broken = {
      create() {
        return {
          Identifier() {
            throw new Error("boom");
          },
        };
      },
    };
    const counting = {
      create(context) {
        return {
          Literal(node) {
            context.report({ node, message: "a literal" });
          },
        };
      },
    };

    const result = run([
      { id: "acme/broken", rule: broken },
      { id: "acme/counting", rule: counting },
    ]);

    expect(result.problems).toEqual(["`acme/broken` threw on app.js: boom"]);
    expect(result.diagnostics.map((diagnostic) => diagnostic.rule)).toEqual(["acme/counting"]);
  });

  it("refuses a fix from a rule that never declared meta.fixable", () => {
    const undeclared = {
      create(context) {
        return {
          Identifier(node) {
            context.report({ node, message: "gone", fix: (fixer) => fixer.remove(node) });
          },
        };
      },
    };

    const result = run([{ id: "acme/undeclared", rule: undeclared }]);

    expect(result.diagnostics).toEqual([]);
    expect(result.problems[0]).toContain("must set `meta.fixable`");
  });

  it("places a loc by ESLint's convention: lines from 1, columns from 0", () => {
    const source = "a;\nbcdef;";
    const at = {
      create(context) {
        return {
          Program() {
            context.report({ loc: { line: 2, column: 3 }, message: "here" });
          },
        };
      },
    };

    const result = run([{ id: "acme/at", rule: at }], source, {
      type: "Program",
      body: [],
      range: [0, source.length],
    });

    expect(result.diagnostics[0].start).toBe(6);
  });
});

describe("helpers", () => {
  it("fills only the placeholders it has data for", () => {
    expect(interpolate("{{a}} and {{ b }}", { a: 1 })).toBe("1 and {{ b }}");
  });

  it("refuses edits that overlap one another", () => {
    expect(() =>
      mergeFixes(
        [
          { range: [0, 3], text: "a" },
          { range: [2, 4], text: "b" },
        ],
        "abcd",
      ),
    ).toThrow();
  });

  it("counts a CRLF as one line break", () => {
    expect(lineStartsOf("a\r\nb\nc")).toEqual([0, 3, 5]);
  });
});

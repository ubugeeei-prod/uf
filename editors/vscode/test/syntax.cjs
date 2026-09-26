const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { Registry, parseRawGrammar } = require("vscode-textmate");
const { loadWASM, OnigScanner, OnigString } = require("vscode-oniguruma");

// Run the real TextMate/Oniguruma tokenizer with the grammars from the VS Code
// under test. Regex-only checks miss declaration state leaking into JSX/body
// code and type names being scoped as function calls.
async function verifySyntax(appRoot) {
  const extension = path.resolve(__dirname, "..");
  const manifest = JSON.parse(fs.readFileSync(path.join(extension, "package.json"), "utf8"));
  const grammars = new Map();
  for (const item of manifest.contributes.grammars) {
    grammars.set(item.scopeName, path.join(extension, item.path));
  }
  for (const name of ["javascript", "typescript-basics"]) {
    const base = path.join(appRoot, "extensions", name);
    const file = path.join(base, "package.json");
    if (!fs.existsSync(file)) continue;
    const builtIn = JSON.parse(fs.readFileSync(file, "utf8"));
    for (const item of builtIn.contributes.grammars) {
      grammars.set(item.scopeName, path.join(base, item.path));
    }
  }
  await loadWASM(fs.readFileSync(require.resolve("vscode-oniguruma/release/onig.wasm")));
  const registry = new Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (patterns) => new OnigScanner(patterns),
      createOnigString: (text) => new OnigString(text),
    }),
    loadGrammar: async (scope) => {
      const file = grammars.get(scope);
      assert.ok(file, `missing grammar ${scope}`);
      return parseRawGrammar(fs.readFileSync(file, "utf8"), file);
    },
    getInjections: (scope) => manifest.contributes.grammars
      .filter((item) => item.injectTo?.includes(scope)).map((item) => item.scopeName),
  });
  const grammar = await registry.loadGrammar("source.js.flow");
  function tokenize(source) {
    let state;
    return source.split("\n").map((line) => {
      const result = grammar.tokenizeLine(line, state);
      state = result.ruleStack;
      return result.tokens.map((token) => ({ text: line.slice(token.startIndex, token.endIndex), scopes: token.scopes }));
    });
  }
  function has(tokens, text, scope) {
    const token = tokens.find((item) => item.text === text);
    assert.ok(token?.scopes.some((name) => name.startsWith(scope)), `${text} should be ${scope}: ${JSON.stringify(tokens)}`);
  }
  const component = tokenize([
    "export component Counter(initial: number) {",
    "  const [count, increment] = useCounter(initial);",
    "  return <button onClick={increment}>{count}</button>;",
    "}",
    "const after = 1;",
  ].join("\n"));
  has(component[0], "Counter", "entity.name.function");
  has(component[0], "initial", "variable.parameter");
  has(component[0], "number", "support.type");
  has(component[1], "count", "variable.other");
  has(component[2], "button", "entity.name.tag");
  has(component[4], "after", "variable.other");
  const hook = tokenize("export hook useCounter(initial: number): [number, () => void] {\n return [initial, () => {}];\n}");
  has(hook[0], "useCounter", "entity.name.function");
  has(hook[0], "initial", "variable.parameter");
  has(hook[0], "void", "support.type");
  const alias = tokenize("opaque type User = {| +id: string, name?: ?string |};\nconst user = { name: 'Ada' };");
  has(alias[0], "User", "entity.name.type");
  has(alias[0], "id", "variable.other.property");
  has(alias[0], "+", "keyword.operator.variance");
  has(alias[1], "user", "variable.other");
  const generic = tokenize("component List<T>(items: $ReadOnlyArray<T>) renders* Item {\n return null;\n}");
  has(generic[0], "$ReadOnlyArray", "entity.name.type");
  has(generic[0], "renders*", "keyword.other.renders");
  has(generic[0], "Item", "entity.name.type");
  // These are the actual markdown signatures uf returns. The registered
  // `flow` language makes their fenced code blocks use this grammar too.
  for (const signature of ["const count: number", "const increment: () => void", "component Counter(initial: number)"]) {
    const tokens = tokenize(signature)[0];
    has(tokens, signature.includes("void") ? "void" : "number", "support.type");
  }
  // Flow's printer uses parenthesized kinds for declarations without a JS
  // keyword. Treating them as JavaScript expressions leaves the type plain.
  for (const signature of ["(parameter) initial: number", "(property) User.age: number", "(method) Counter.reset(): void"]) {
    has(tokenize(signature)[0], signature.includes("void") ? "void" : "number", "support.type");
  }
  const plain = tokenize("const text = 'component Fake(x: number)'; // hook useFake(x: any)");
  assert.ok(plain[0].every((token) => !token.scopes.includes("meta.function.flow")));
  function themeRules(file) {
    const theme = JSON.parse(fs.readFileSync(file, "utf8"));
    return [...(theme.include ? themeRules(path.resolve(path.dirname(file), theme.include)) : []), ...(theme.tokenColors ?? [])];
  }
  // Check real built-in dark/light theme colors as well as scopes. A grammar
  // may produce a named scope that no theme colors, which still looks plain.
  for (const name of ["dark_plus", "light_plus"]) {
    registry.setTheme({ settings: themeRules(path.join(appRoot, "extensions/theme-defaults/themes", `${name}.json`)) });
    function color(line, text) {
      const at = line.indexOf(text);
      const tokens = grammar.tokenizeLine2(line).tokens;
      for (let i = 0; i < tokens.length; i += 2) {
        if (tokens[i] <= at && (i + 2 === tokens.length || tokens[i + 2] > at)) {
          // TextMate EncodedTokenAttributes' foreground occupies bits 15..23.
          return registry.getColorMap()[(tokens[i + 1] >>> 15) & 0x1ff];
        }
      }
      throw new Error(`no color for ${text}`);
    }
    const signature = "component Counter(initial: number)";
    assert.notEqual(color(signature, "Counter"), color(signature, "initial"), `${name}: function versus parameter`);
    assert.notEqual(color(signature, "number"), color(signature, "initial"), `${name}: type versus parameter`);
    const hover = "const count: number";
    assert.notEqual(color(hover, "count"), color(hover, "number"), `${name}: hover binding versus type`);
    const parameter = "(parameter) initial: number";
    assert.notEqual(color(parameter, "initial"), color(parameter, "number"), `${name}: parameter hover versus type`);
  }
  registry.dispose();
  console.log("Flow source, JSX, exact types, generics, and hover signatures passed TextMate tokenization");
}

module.exports = { verifySyntax };
if (require.main === module) verifySyntax(process.argv[2]).catch((error) => { console.error(error); process.exitCode = 1; });

// @noflow
//
// ox-content owns the Markdown parse. The MDX compiler owns React codegen;
// Acorn supplies the ESTree payloads it requires for embedded JavaScript.
// Neither the ox-content site generator nor a default theme enters the page.

import { transformMdast } from "@ox-content/napi";
import { Parser } from "acorn";
import jsx from "acorn-jsx";

const JavaScript = Parser.extend(jsx());
const OPTIONS = { ecmaVersion: "latest", sourceType: "module" };

/** The remark parser, backed by ox-content's native Markdown/MDX AST. */
export default function remarkOxContent() {
  this.parser = (source, file) => {
    const result = transformMdast(source, {
      gfm: true,
      mdx: !file.path?.endsWith(".md"),
      frontmatter: true,
    });
    if (result.errors.length !== 0) file.fail(result.errors.join("; "));
    file.data.ufFrontmatter = /^---\r?\n/.test(source) ? JSON.parse(result.frontmatter) : undefined;
    const tree = JSON.parse(result.astJson);
    try {
      attachJavaScript(tree);
    } catch (error) {
      file.fail(`Invalid JavaScript in MDX: ${error.message}`);
    }
    return tree;
  };
}

function expression(value, spread) {
  // A comment-only expression has no value. Acorn's tokenizer skips comments,
  // without a second parse or a hand-written JavaScript lexer.
  if (JavaScript.tokenizer(value, OPTIONS).getToken().type.label === "eof") {
    return { type: "Program", body: [], sourceType: "module" };
  }
  const source = spread ? `{${value}}` : value;
  return JavaScript.parse(`(\n${source}\n)`, OPTIONS);
}

function attachJavaScript(node) {
  if (node.type === "mdxjsEsm") {
    node.data = { ...node.data, estree: JavaScript.parse(node.value, OPTIONS) };
  } else if (
    node.type === "mdxFlowExpression" ||
    node.type === "mdxTextExpression" ||
    node.type === "mdxJsxAttributeValueExpression" ||
    node.type === "mdxJsxExpressionAttribute"
  ) {
    node.data = {
      ...node.data,
      estree: expression(node.value, node.type === "mdxJsxExpressionAttribute"),
    };
  }
  for (const child of node.children ?? []) attachJavaScript(child);
  for (const attribute of node.attributes ?? []) {
    attachJavaScript(attribute);
    if (typeof attribute.value === "object" && attribute.value !== null) {
      attachJavaScript(attribute.value);
    }
  }
}

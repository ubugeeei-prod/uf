// @noflow
//
// A document's YAML front matter, as `export const frontmatter`.
//
// Plain JavaScript, for the reason `index.js` gives: Vite imports this before
// any transform runs.
//
// This is `remark-mdx-frontmatter` for the one format uf reads, and it replaces
// that package for one reason. The package imported `toml` 3.0.0 at the top of
// its module whether or not a document held any TOML, so every project uf
// scaffolded installed a parser with two high advisories and failed its first
// `uf audit` (#1009). The TOML half was never reachable from here either:
// `remark-frontmatter` is given its default, which recognises YAML and nothing
// else, so a `+++` block was never a front-matter node to parse.
//
// For YAML the output is the package's, through the same two helpers it used:
// the first `yaml` node is parsed and defined as `frontmatter`, and a document
// with none exports `undefined`.

import { valueToEstree } from "estree-util-value-to-estree";
import { define } from "unist-util-mdx-define";
import { parse } from "yaml";

/** The remark plugin: `export const frontmatter` from a document's YAML. */
export default function remarkFrontmatterExport() {
  return (tree, file) => {
    const node = tree.children.find((child) => child.type === "yaml");
    const data = node == null ? undefined : parse(node.value);
    define(tree, file, {
      frontmatter: valueToEstree(data, { preserveReferences: true }),
    });
  };
}

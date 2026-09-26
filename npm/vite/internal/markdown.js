// @noflow
// ox-content parses plain Markdown and frontmatter. MDX keeps the official
// compiler's JavaScript/JSX grammar, including lowercase tags and nested Markdown.
// uf owns the page shell and theme in both cases.
import { parseFrontmatter, transformMdast } from "@ox-content/napi";

/** Native Markdown/frontmatter with the official MDX parser for JSX documents. */
export default function remarkOxContent() {
  const mdxParser = this.parser;
  this.parser = (source, file) => {
    const hasFrontmatter = /^---\r?\n/.test(source);
    if (!file.path?.endsWith(".md")) {
      const result = parseFrontmatter(source);
      file.data.ufFrontmatter = hasFrontmatter ? result.frontmatter : undefined;
      // Retain the original line numbers for MDX errors and source maps.
      const prefix = source.slice(0, source.length - result.content.length);
      return mdxParser(prefix.replace(/[^\r\n]/g, "") + result.content, file);
    }
    const result = transformMdast(source, { gfm: true, mdx: false, frontmatter: true });
    if (result.errors.length !== 0) file.fail(result.errors.join("; "));
    file.data.ufFrontmatter = hasFrontmatter ? JSON.parse(result.frontmatter) : undefined;
    const tree = JSON.parse(result.astJson);
    escapeHtml(tree);
    return tree;
  };
}

// Plain Markdown does not execute JSX or raw HTML. Render HTML as literal text,
// as uf doc does, so the React compiler never receives unsupported HAST raw nodes.
function escapeHtml(node) {
  if (node.type === "html") node.type = "text";
  for (const child of node.children ?? []) escapeHtml(child);
}

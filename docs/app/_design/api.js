// @flow
//
// The API reference: every export of every published package, as the package
// declares it.
//
// The data is written by `tools/docs/api.js` before the site is built — one
// JSON file per package in `docs/.generated/api/`, from `uf doc --json` and
// the packages' `exports` — and read here through `import.meta.glob`, so a
// checkout that has not run it yet renders an empty reference rather than
// failing to resolve an import. Nothing in it is written by hand, which is the
// point: a signature on these pages is the signature in the source.
//
// A server module. The pages are prerendered, and the data never reaches the
// browser except as the HTML it became.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import { blocksOf } from "./doc-comment.js";

type Entry = {|
  readonly name: string,
  readonly kind: string,
  readonly signature: string,
  readonly description: string,
|};

type Bare = {|
  readonly name: string,
  readonly kind: string,
  readonly file: string,
  readonly line: number,
|};

type Module = {|
  readonly specifier: string,
  readonly files: $ReadOnlyArray<string>,
  readonly entries: $ReadOnlyArray<Entry>,
  readonly bare: $ReadOnlyArray<Bare>,
  readonly everythingFrom: $ReadOnlyArray<string>,
|};

export type ApiPackage = {|
  readonly name: string,
  readonly slug: string,
  readonly dir: string,
  readonly version: string,
  readonly description: string,
  readonly modules: $ReadOnlyArray<Module>,
|};

const SOURCE = "https://github.com/ubugeeei-prod/uf/blob/main/npm/";

const FILES = import.meta.glob<{ readonly default: ApiPackage, ... }>(
  "../../.generated/api/*.json",
  { eager: true },
);

/** Every package with a reference page, by name. */
export const packages: $ReadOnlyArray<ApiPackage> = Object.keys(FILES)
  .map((file) => FILES[file].default)
  .sort((a, b) => a.name.localeCompare(b.name));

/** The package whose page is `/reference/api/<slug>`, or `null`. */
export function packageFor(slug: string): ApiPackage | null {
  return packages.find((item) => item.slug === slug) ?? null;
}

function counts(item: ApiPackage): {| documented: number, bare: number |} {
  let documented = 0;
  let bare = 0;
  for (const module of item.modules) {
    documented += module.entries.length;
    bare += module.bare.length;
  }
  return { documented, bare };
}

/** A heading id from a specifier and, under it, a name. */
function anchor(...parts: $ReadOnlyArray<string>): string {
  return parts
    .join("-")
    .toLowerCase()
    .replace(/^@uniflowed\//, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

/** The list on the reference's landing page: every package, and how much of it has a doc comment. */
export component ApiIndex() {
  if (packages.length === 0) {
    return (
      <p>
        The reference is written when the site is built — <code>uf run docs:build</code> — and this
        build has not written it.
      </p>
    );
  }
  return (
    <table className="api-index">
      <thead>
        <tr>
          <th>Package</th>
          <th>What it is</th>
          <th>Exports</th>
        </tr>
      </thead>
      <tbody>
        {packages.map((item) => {
          const { documented, bare } = counts(item);
          return (
            <tr key={item.name}>
              <td>
                <Link to={`/reference/api/${item.slug}`}>
                  <code>{item.name}</code>
                </Link>
              </td>
              <td>{item.description}</td>
              <td className="api-count">
                {documented + bare === 0 ? "re-exports" : `${documented} of ${documented + bare}`}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/** One package's reference. */
export component ApiReference(item: ApiPackage) {
  const { documented, bare } = counts(item);
  const dir = item.dir;
  return (
    <>
      <p className="eyebrow">API reference</p>
      <h1 id={anchor(item.slug)}>{item.name}</h1>
      <div className="lede">
        <p>{item.description}</p>
      </div>
      <p className="api-meta">
        <span>{item.version}</span>
        <span>
          {documented} documented {documented === 1 ? "export" : "exports"}
          {bare > 0 ? `, ${bare} without a doc comment` : ""}
        </span>
        <a href={`${SOURCE}${dir}`}>
          <code>npm/{dir}</code>
        </a>
      </p>
      <p className="api-note">
        Written from the source by{" "}
        <Link to="/reference/cli#uf-doc">
          <code>uf doc</code>
        </Link>{" "}
        when this site was built: the signature and the comment above each export, grouped by the
        specifier a program imports it from.
      </p>
      {item.modules.map((module) => (
        <section key={module.specifier} className="api-module">
          <h2 id={anchor(module.specifier)}>
            <code>{module.specifier}</code>
          </h2>
          {module.everythingFrom.length > 0 ? (
            <p>
              Re-exports everything from{" "}
              {module.everythingFrom.map((specifier, index) => (
                <React.Fragment key={specifier}>
                  {index > 0 ? ", " : null}
                  <code>{specifier}</code>
                </React.Fragment>
              ))}
              .
            </p>
          ) : null}
          {module.entries.map((entry) => (
            <article key={`${entry.kind}:${entry.name}`} className="api-entry">
              <p className="api-kind">{entry.kind}</p>
              <h3 id={anchor(module.specifier, entry.name)}>
                <code>{entry.name}</code>
              </h3>
              <pre className="api-signature">
                <code>{entry.signature}</code>
              </pre>
              <Prose text={entry.description} />
            </article>
          ))}
          {module.bare.length > 0 ? (
            <>
              <p className="api-bare-title">Without a doc comment</p>
              <ul className="api-bare">
                {module.bare.map((entry) => (
                  <li key={`${entry.kind}:${entry.name}`}>
                    <code>{entry.name}</code>
                    <span className="api-kind">{entry.kind}</span>
                    {entry.kind.startsWith("from ") ? null : (
                      <a href={`${SOURCE}${dir}/${entry.file}#L${entry.line}`}>
                        {entry.file}:{entry.line}
                      </a>
                    )}
                  </li>
                ))}
              </ul>
            </>
          ) : null}
        </section>
      ))}
    </>
  );
}

/**
 * A doc comment, rendered. The comments are Markdown in practice — paragraphs,
 * `code`, fenced blocks and lists — and that is all this reads; anything else
 * stays as its text. React elements rather than HTML, so a comment cannot
 * inject markup into the page.
 */
component Prose(text: string) {
  const blocks = text.trim() === "" ? [] : blocksOf(text);
  return (
    <>
      {blocks.map((block, index) =>
        block.kind === "code" ? (
          // A comment's blocks have no identity beyond their order.
          // uf-lint-disable-next-line react/no-array-index-key
          <pre key={index}>
            <code>{block.text}</code>
          </pre>
        ) : block.kind === "list" ? (
          // uf-lint-disable-next-line react/no-array-index-key
          <ul key={index}>
            {block.items.map((line) => (
              <li key={line}>
                <Inline text={line} />
              </li>
            ))}
          </ul>
        ) : (
          // uf-lint-disable-next-line react/no-array-index-key
          <p key={index}>
            <Inline text={block.text} />
          </p>
        ),
      )}
    </>
  );
}

/** `code` spans inside running text; everything else is text. */
component Inline(text: string) {
  const parts: Array<{| text: string, start: number |}> = [];
  let start = 0;
  for (const part of text.split(/(`[^`]+`)/)) {
    parts.push({ text: part, start });
    start += part.length;
  }
  return (
    <>
      {parts.map((part) =>
        part.text.startsWith("`") && part.text.endsWith("`") && part.text.length > 1 ? (
          <code key={part.start}>{part.text.slice(1, -1)}</code>
        ) : (
          <React.Fragment key={part.start}>{part.text}</React.Fragment>
        ),
      )}
    </>
  );
}

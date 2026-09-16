// Emit uf_lint's ARIA table as Rust, from `aria-query` — the encoding of
// WAI-ARIA 1.2 that eslint-plugin-jsx-a11y itself reads, so that a uf rule the
// formatting and linting guide sends to a plugin rule answers the same
// question from the same data.
//
// Run it with `aria-query` resolvable, and write the result over the generated
// table:
//
//   npm pack aria-query && tar -xzf aria-query-*.tgz
//   NODE_PATH=. node tools/aria/gen-table.cjs \
//     > crates/uf_lint/src/runner/tree/aria/table.rs
//
// `--role-elements` also emits the role-to-element table, and `--reserved` the
// elements ARIA reserves; only the rules that read a role against an element
// need either.
const q = require("aria-query");
const version = require("aria-query/package.json").version;

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

// aria-query 5.3.2 stops at WAI-ARIA 1.2 plus the 1.3 names browsers ship. The
// two index-text names are the rest of that 1.3 set, and `a11y/aria-props`
// already accepts them, so the table has to know them too or a rule would
// report an attribute the sibling rule calls real. Both are plain strings.
// https://w3c.github.io/aria/#aria-colindextext
const EXTRA = {
  "aria-colindextext": { type: "string" },
  "aria-rowindextext": { type: "string" },
};

const fromAriaQuery = Object.fromEntries(q.aria.keys().map((name) => [name, q.aria.get(name)]));
const attributes = { ...fromAriaQuery, ...EXTRA };
const attributeNames = Object.keys(attributes).sort();
const attributeIndex = new Map(attributeNames.map((name, at) => [name, at]));
if (attributeNames.length > 64) throw new Error("mask no longer fits in a u64");

const KIND = {
  boolean: () => "Kind::Boolean",
  tristate: () => "Kind::Tristate",
  integer: () => "Kind::Integer",
  number: () => "Kind::Number",
  string: () => "Kind::Text",
  id: () => "Kind::Id",
  idlist: () => "Kind::IdList",
  token: (spec) => `Kind::Token(&[${tokens(spec)}])`,
  tokenlist: (spec) => `Kind::TokenList(&[${tokens(spec)}])`,
};

// `aria-current` and friends list `true`/`false` among their tokens; those are
// the boolean spellings of the same attribute and are handled by the kind.
const tokens = (spec) =>
  (spec.values || [])
    .filter((value) => typeof value === "string" && value !== "undefined")
    .map((value) => JSON.stringify(value))
    .join(", ");

// A token attribute that also lists true/false takes the boolean spellings.
const takesBoolean = (spec) => (spec.values || []).some((value) => typeof value === "boolean");
// `aria-orientation`'s "undefined" is a real token in the spec.
const takesUndefined = (spec) => (spec.values || []).includes("undefined");

const mask = (names) => {
  let bits = 0n;
  for (const name of names) {
    const at = attributeIndex.get(name);
    if (at === undefined) continue;
    bits |= 1n << BigInt(at);
  }
  return `0x${bits.toString(16)}`;
};

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

const roleNames = q.roles.keys().slice().sort();
const isA = (name, ancestor) =>
  (q.roles.get(name).superClass || []).some((chain) => chain.includes(ancestor));

// WAI-ARIA 1.2, "Global States and Properties": every role takes these unless
// it prohibits them, and `aria-query` hangs them off the abstract `roletype`.
const GLOBAL = Object.keys(q.roles.get("roletype").props || {});

// What a role takes: its own properties, everything it inherits, and the
// globals.
//
// Most roles inline the whole set, but not all of them do. `none` carries no
// properties *and* no superclass, and `doc-pullquote` inherits only from
// `none` — so a mask built from `props` alone left both supporting nothing,
// and `a11y/role-supports-aria-props` would have rejected `aria-hidden` on
// `<div role="none">`, which is working markup.
const supportedProps = (name, seen = new Set()) => {
  if (seen.has(name)) return [];
  seen.add(name);
  const role = q.roles.get(name);
  if (!role) return [];
  const inherited = (role.superClass || []).flatMap((chain) =>
    chain.flatMap((ancestor) => supportedProps(ancestor, seen)),
  );
  return [...Object.keys(role.props || {}), ...inherited, ...GLOBAL];
};

const roleRow = (name) => {
  const role = q.roles.get(name);
  const supported = supportedProps(name);
  // A role an author may write that takes nothing at all is a table bug, not a
  // fact about ARIA: fail the generation rather than ship it.
  if (!role.abstract && supported.length === 0) {
    throw new Error(`role ${name} would support no attribute at all`);
  }
  const required = Object.keys(role.requiredProps || {});
  const prohibited = role.prohibitedProps || [];
  const flags = [];
  if (role.abstract) flags.push("ABSTRACT");
  if (isA(name, "composite")) flags.push("COMPOSITE");
  if (isA(name, "widget")) flags.push("WIDGET");
  if ((role.requiredContextRole || role.requireContextRole || []).length > 0)
    flags.push("CONTEXTUAL");
  return `    Role {
        name: ${JSON.stringify(name)},
        flags: ${flags.length ? flags.map((flag) => `Flags::${flag}`).join(".union(") + ")".repeat(flags.length - 1) : "Flags::NONE"},
        supported: ${mask(supported)},
        required: ${mask(required)},
        prohibited: ${mask(prohibited)},
    },`;
};

// ---------------------------------------------------------------------------
// Implicit roles (HTML-AAM, as aria-query encodes it)
// ---------------------------------------------------------------------------

// An entry whose concept carries `constraints` depends on where the element
// sits — "scoped to the body element", "direct descendant of ul" — which is a
// question about the document, not about the element. uf keeps those and marks
// them, so a rule can decide for itself whether to answer.
const implicit = [];
for (const [concept, roles] of q.elementRoles.entries()) {
  const role = Array.from(roles)[0];
  if (!role) continue;
  implicit.push({
    element: concept.name,
    attributes: (concept.attributes || []).map((attribute) => ({
      name: attribute.name,
      value: attribute.value === undefined ? null : String(attribute.value),
      set: (attribute.constraints || []).includes("set"),
      undefinedConstraint: (attribute.constraints || []).includes("undefined"),
    })),
    constrained: (concept.constraints || []).length > 0,
    role,
  });
}
// Most specific first: an entry with more attribute constraints wins.
implicit.sort((left, right) =>
  left.element === right.element
    ? right.attributes.length - left.attributes.length
    : left.element < right.element
      ? -1
      : 1,
);

const implicitRow = (entry) => {
  const attributes = entry.attributes
    .map(
      (attribute) =>
        `Required { name: ${JSON.stringify(attribute.name)}, value: ${
          attribute.value === null ? "None" : `Some(${JSON.stringify(attribute.value)})`
        }, set: ${attribute.set}, unset: ${attribute.undefinedConstraint} }`,
    )
    .join(", ");
  return `    Implicit {
        element: ${JSON.stringify(entry.element)},
        attributes: &[${attributes}],
        placed: ${entry.constrained},
        role: ${JSON.stringify(entry.role)},
    },`;
};

// ---------------------------------------------------------------------------
// Role -> elements (prefer-tag-over-role)
// ---------------------------------------------------------------------------

const roleElements = [];
for (const [role, concepts] of q.roleElements.entries()) {
  const tags = Array.from(concepts).map((concept) => ({
    name: concept.name,
    attributes: (concept.attributes || []).map((attribute) => ({
      name: attribute.name,
      value: attribute.value === undefined ? null : String(attribute.value),
    })),
  }));
  roleElements.push({ role, tags });
}
roleElements.sort((left, right) => (left.role < right.role ? -1 : 1));

const tagRow = (entry) => {
  const tags = entry.tags
    .map((tag) => {
      const attributes = tag.attributes
        .map(
          (attribute) =>
            `Attr { name: ${JSON.stringify(attribute.name)}, value: ${
              attribute.value === null ? "None" : `Some(${JSON.stringify(attribute.value)})`
            } }`,
        )
        .join(", ");
      return `Tag { name: ${JSON.stringify(tag.name)}, attributes: &[${attributes}] }`;
    })
    .join(", ");
  return `    (${JSON.stringify(entry.role)}, &[${tags}]),`;
};

// ---------------------------------------------------------------------------
// Emit
// ---------------------------------------------------------------------------

const out = [];
out.push(`//! The ARIA table: roles, the attributes each one takes, and the roles`);
out.push(`//! HTML elements carry on their own.`);
out.push(`//!`);
out.push(`//! **Generated — do not edit by hand.** \`tools/aria/gen-table.cjs\``);
out.push(`//! emits this file from \`aria-query\` ${version}, which is the encoding of`);
out.push(`//! [WAI-ARIA 1.2][aria] that \`eslint-plugin-jsx-a11y\` itself reads, plus the`);
out.push(`//! two ARIA 1.3 index-text attributes \`a11y/aria-props\` already accepts.`);
out.push(`//!`);
out.push(`//! [aria]: https://www.w3.org/TR/wai-aria-1.2/`);
out.push(``);
// The role -> element table is only read by the rules that suggest a tag in
// place of a role, so it is emitted on demand rather than left in the crate
// with nothing reading it.
const withRoleElements = process.argv.includes("--role-elements");
out.push(
  withRoleElements
    ? `use super::{Attr, Flags, Implicit, Kind, Required, Role, Spec, Tag};`
    : `use super::{Flags, Implicit, Kind, Required, Role, Spec};`,
);
out.push(``);
out.push(`/// Every ARIA attribute, sorted by name. The index of a name in this table`);
out.push(`/// is its bit in a role's attribute masks.`);
out.push(`pub(super) static ATTRIBUTES: &[Spec] = &[`);
for (const name of attributeNames) {
  const spec = attributes[name];
  const kind = KIND[spec.type];
  if (!kind) throw new Error(`unknown attribute type ${spec.type} for ${name}`);
  out.push(`    Spec {
        name: ${JSON.stringify(name)},
        kind: ${kind(spec)},
        boolean_spelling: ${takesBoolean(spec)},
        undefined_token: ${takesUndefined(spec)},
    },`);
}
out.push(`];`);
out.push(``);
out.push(`/// Every ARIA role, sorted by name.`);
out.push(`pub(super) static ROLES: &[Role] = &[`);
for (const name of roleNames) out.push(roleRow(name));
out.push(`];`);
out.push(``);
out.push(`/// What role an HTML element carries without being told, most specific first.`);
out.push(`pub(super) static IMPLICIT_ROLES: &[Implicit] = &[`);
for (const entry of implicit) out.push(implicitRow(entry));
out.push(`];`);
out.push(``);
if (withRoleElements) {
  out.push(`/// The HTML elements that already are each role.`);
  out.push(`pub(super) static ROLE_ELEMENTS: &[(&str, &[Tag])] = &[`);
  for (const entry of roleElements) out.push(tagRow(entry));
  out.push(`];`);
  out.push(``);
}

if (process.argv.includes("--reserved")) {
  const reserved = [];
  for (const [name, spec] of q.dom.entries()) {
    if (spec && spec.reserved) reserved.push(name);
  }
  reserved.sort();
  out.push(`/// The HTML elements ARIA reserves.`);
  out.push(`///`);
  out.push(`/// None of them is rendered, so none of them is in the accessibility tree`);
  out.push(`/// for a role or an \`aria-*\` to say anything about.`);
  out.push(`pub(super) static RESERVED_ELEMENTS: &[&str] = &[`);
  for (const name of reserved) out.push(`    ${JSON.stringify(name)},`);
  out.push(`];`);
  out.push(``);
}

process.stdout.write(out.join("\n"));

process.stderr.write(
  `attributes: ${attributeNames.length}\nroles: ${roleNames.length}\nimplicit: ${implicit.length}\nroleElements: ${roleElements.length}\naria-query: ${version}\n`,
);

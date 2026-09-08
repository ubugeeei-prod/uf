// @flow
//
// The accessibility audit behind `expect(container).toHaveNoAxeViolations()`.
//
// `@uniflowed/react-testing` renders into a real DOM, which is the expensive
// half of an accessibility assertion — and nothing was reading it. axe-core is
// the engine every other testing stack uses for that, it is the engine the
// browsers' own developer tools use, and reimplementing its four hundred-odd
// checks would be a small lookalike of exactly the kind uf's guide says not to
// ship. So uf runs axe-core rather than competing with it. See
// ubugeeei-prod/uf#511.
//
// # Why the import is dynamic, and why the specifier is a constant
//
// Dynamic, because `@uniflowed/test` is imported by every test file in every
// project and most of them will never audit anything: a static import would
// put a megabyte of rule definitions into the start-up of every worker to
// serve the files that ask for it. It is declared as an *optional* peer
// dependency for the same reason — a project that never writes the matcher
// should not be made to install the engine — and a project that writes it
// without installing it is told so in one sentence rather than by
// `ERR_MODULE_NOT_FOUND` from inside a matcher.
//
// A constant specifier, because the alternative is a module path read from
// configuration and handed to `import()`. `docs/security.md` is about exactly
// that shape: `uf.config.js` is a file a cloned repository brings with it, and
// "which module does the test runner load" is not a question it should get to
// answer. What a project *may* configure is which rules run, which is data.
//
// # What is configured once rather than per test
//
// The rule set. A project that has decided `color-contrast` cannot be judged
// by a DOM with no layout has decided it for the whole suite, and repeating
// that decision in every assertion is how two tests come to disagree about
// what "accessible" means. `uf.config.js`'s `accessibility.axe` block is the
// answer, delivered to workers in the environment (`UF_AXE`) the same way
// `UF_UPDATE_SNAPSHOTS` delivers the other run-wide decision. A call may still
// pass overrides, and they are merged over the project's — narrowing one
// assertion is legitimate, and it is visible on the line that does it.

/** How serious a violation is, weakest first. */
export type AxeImpact = "minor" | "moderate" | "serious" | "critical";

/** What a test or a project may say about a run. */
export type AxeOptions = {
  /** Run only rules carrying one of these axe tags; all rules when empty. */
  readonly tags?: $ReadOnlyArray<string>,
  /** Rule ids to turn off, by id. */
  readonly disabledRules?: $ReadOnlyArray<string>,
  /** Weakest impact that counts as a failure. */
  readonly minImpact?: AxeImpact,
};

/** One thing axe found, reduced to what a failure message needs. */
export type AxeViolation = {
  readonly id: string,
  readonly impact: AxeImpact | null,
  readonly help: string,
  readonly helpUrl: string,
  readonly nodes: $ReadOnlyArray<string>,
};

/** Impacts in order, so a floor can be compared rather than matched. */
const IMPACTS: $ReadOnlyArray<AxeImpact> = ["minor", "moderate", "serious", "critical"];

/** Most violations one message names; the rest are counted. */
const MAX_VIOLATIONS_SHOWN = 10;

/** Most offending elements one violation names. */
const MAX_NODES_SHOWN = 3;

/** Longest excerpt of one element's markup a message quotes. */
const MAX_NODE_CHARS = 120;

/**
 * The engine, loaded at most once per process.
 *
 * A promise rather than a module, so two assertions in flight at the same time
 * share one load rather than racing to start two.
 */
let engine: Promise<$FlowFixMe> | null = null;

/**
 * axe-core, or a refusal that says what to install.
 *
 * The `$FlowFixMe` is the module's own shape: axe-core ships TypeScript
 * declarations and no Flow ones, so what comes back is untyped whatever this
 * file writes. It is confined to this one binding — everything the rest of the
 * module reads out of a result goes through the readers below, which narrow
 * rather than assert.
 */
function axeEngine(): Promise<$FlowFixMe> {
  if (engine == null) {
    engine = import("axe-core").then(
      (module) => {
        const loaded: $FlowFixMe = (module as $FlowFixMe).default ?? module;
        if (loaded == null || typeof loaded.run !== "function") {
          throw new Error("`axe-core` is installed but has no `run`, so uf cannot audit with it");
        }
        return loaded;
      },
      () => {
        throw new Error(
          "`toHaveNoAxeViolations` needs axe-core, which this project does not have. " +
            "Add it — `uf add -D axe-core` — and the matcher, and `uf dev`'s audit, " +
            "start working; uf does not install it for you because a project that " +
            "never audits should not carry the engine.",
        );
      },
    );
  }
  return engine;
}

/** A string field, or `""`. */
function stringAt(value: mixed, key: string): string {
  if (value == null || typeof value !== "object") return "";
  const found = value[key];
  return typeof found === "string" ? found : "";
}

/** An array field, or an empty one. */
function arrayAt(value: mixed, key: string): $ReadOnlyArray<mixed> {
  if (value == null || typeof value !== "object") return [];
  const found = value[key];
  return Array.isArray(found) ? found : [];
}

/** The impact named at `key`, when it is one of the four. */
function impactAt(value: mixed, key: string): AxeImpact | null {
  const impact = stringAt(value, key);
  return IMPACTS.find((known) => known === impact) ?? null;
}

/** The impact axe gave a violation, when it gave one it knows. */
function impactOf(value: mixed): AxeImpact | null {
  return impactAt(value, "impact");
}

/**
 * The project's settings, read once from the environment.
 *
 * Absent, unreadable or the wrong shape all mean the same thing: no project
 * settings, run every rule. A malformed value is not worth failing a suite
 * over — the setting is a narrowing, and the widest answer is the safe one.
 */
function projectOptions(): AxeOptions {
  const raw =
    typeof process === "undefined" ? undefined : ((process.env as $FlowFixMe)?.UF_AXE ?? undefined);
  if (typeof raw !== "string" || raw === "") return {};
  let parsed: mixed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (parsed == null || typeof parsed !== "object") return {};
  const tags = arrayAt(parsed, "tags").filter((tag) => typeof tag === "string");
  const disabled = arrayAt(parsed, "disabledRules").filter((rule) => typeof rule === "string");
  const impact = impactAt(parsed, "minImpact") ?? undefined;
  return {
    tags: tags as $FlowFixMe,
    disabledRules: disabled as $FlowFixMe,
    minImpact: impact,
  };
}

/** The project's settings, with a call's own merged over them. */
function resolveOptions(overrides: AxeOptions | void): AxeOptions {
  const project = projectOptions();
  return {
    tags: overrides?.tags ?? project.tags,
    disabledRules: overrides?.disabledRules ?? project.disabledRules,
    minImpact: overrides?.minImpact ?? project.minImpact,
  };
}

/** Those settings in axe's own vocabulary. */
function axeRunOptions(options: AxeOptions): { [string]: mixed } {
  const run: { [string]: mixed } = {};
  const tags = options.tags ?? [];
  if (tags.length > 0) run.runOnly = { type: "tag", values: [...tags] };
  const disabled = options.disabledRules ?? [];
  if (disabled.length > 0) {
    const rules: { [string]: mixed } = {};
    for (const rule of disabled) rules[rule] = { enabled: false };
    run.rules = rules;
  }
  return run;
}

/**
 * Whether a violation is at or above the configured floor.
 *
 * A violation axe could not rate is kept: "we do not know how bad this is" is
 * not a reason to drop it, and dropping it is how a floor set to `critical`
 * would quietly hide everything unrated.
 */
function meetsFloor(violation: AxeViolation, floor: AxeImpact | void): boolean {
  if (floor == null || violation.impact == null) return true;
  return IMPACTS.indexOf(violation.impact) >= IMPACTS.indexOf(floor);
}

/** One element, as short a piece of markup as still identifies it. */
function excerpt(html: string): string {
  const line = html.split("\n")[0].trim();
  return line.length > MAX_NODE_CHARS ? `${line.slice(0, MAX_NODE_CHARS)}…` : line;
}

/**
 * Audit `node` and report what it found, weakest results already dropped.
 *
 * Rejects when axe-core is not installed or the host has no document to audit.
 * Both are "this assertion cannot be made here" rather than "this assertion
 * failed", and both are ordinary errors rather than an [`AssertionError`]: the
 * runner prints an assertion failure as a comparison, and there is nothing
 * here to compare — what a reader needs is the sentence saying what to
 * install.
 */
export async function auditElement(
  node: mixed,
  overrides?: AxeOptions,
): Promise<$ReadOnlyArray<AxeViolation>> {
  if (typeof globalThis.document === "undefined") {
    throw new Error(
      "`toHaveNoAxeViolations` needs a document, and this host has none. " +
        "Render with `@uniflowed/react-testing`, or run the file with `uf test --browser`.",
    );
  }
  const axe = await axeEngine();
  const options = resolveOptions(overrides);
  const results = await axe.run(node, axeRunOptions(options));
  const found: Array<AxeViolation> = [];
  for (const raw of arrayAt(results, "violations")) {
    const violation: AxeViolation = {
      id: stringAt(raw, "id"),
      impact: impactOf(raw),
      help: stringAt(raw, "help"),
      helpUrl: stringAt(raw, "helpUrl"),
      nodes: arrayAt(raw, "nodes").map((one) => excerpt(stringAt(one, "html"))),
    };
    if (meetsFloor(violation, options.minImpact)) found.push(violation);
  }
  return found;
}

/**
 * What a reader is told when the audit found something.
 *
 * The rule id, what it wants, the elements that broke it, and the page Deque
 * publishes about it — which is the part that turns "aria-required-children"
 * into something a person can act on without a search engine.
 *
 * Bounded, because a container rendered from bad markup can violate a hundred
 * rules and a failure that fills a terminal is a failure nobody reads.
 */
export function describeViolations(violations: $ReadOnlyArray<AxeViolation>): string {
  const lines: Array<string> = [];
  for (const violation of violations.slice(0, MAX_VIOLATIONS_SHOWN)) {
    const impact = violation.impact == null ? "" : ` (${violation.impact})`;
    lines.push(`  ${violation.id}${impact} — ${violation.help}`);
    for (const node of violation.nodes.slice(0, MAX_NODES_SHOWN)) {
      lines.push(`    ${node}`);
    }
    if (violation.nodes.length > MAX_NODES_SHOWN) {
      lines.push(`    …and ${violation.nodes.length - MAX_NODES_SHOWN} more elements`);
    }
    if (violation.helpUrl !== "") lines.push(`    ${violation.helpUrl}`);
  }
  if (violations.length > MAX_VIOLATIONS_SHOWN) {
    lines.push(`  …and ${violations.length - MAX_VIOLATIONS_SHOWN} more rules`);
  }
  return lines.join("\n");
}

/** The rule ids a set of violations covers, for the `expected`/`received` pair. */
export function violationIds(violations: $ReadOnlyArray<AxeViolation>): string {
  return violations.map((violation) => violation.id).join(", ");
}

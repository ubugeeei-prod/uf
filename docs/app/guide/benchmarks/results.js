// @flow
//
// The benchmark run committed as `tools/bench/toolchain/baseline.json`,
// rendered: the machine, the versions, and every row with its command.
//
// Read when the site is built, from the same file the nightly regression job
// compares against, so the numbers on the page are the numbers the gate holds
// uf to. Nothing here is typed in by hand. The file is written by
// `tools/bench/toolchain/bench.js` and documented in `report.js` beside it;
// this reads the fields it renders and checks each one, so a file of another
// shape fails the build with a sentence rather than rendering a blank.

import * as React from "@uniflowed/react";

/** The committed run, as text, by path: one entry, or none before the first. */
const FILES = import.meta.glob<string>("../../../../tools/bench/toolchain/baseline.json", {
  eager: true,
  import: "default",
  query: "?raw",
});

type Cpu = {| readonly median: number |};

type Row = {|
  readonly tool: string,
  readonly stage: string,
  readonly fixture: string,
  readonly title: string,
  readonly command: string,
  readonly cache: string,
  readonly runs: number,
  readonly median: number,
  readonly min: number,
  readonly max: number,
  readonly cpu: Cpu | null,
|};

type Skipped = {| readonly tool: string, readonly stage: string | null, readonly reason: string |};

type Run = {|
  readonly finishedAt: string,
  readonly provisional: boolean,
  readonly machine: {|
    readonly platform: string,
    readonly arch: string,
    readonly cpu: string,
    readonly cores: number,
    readonly memoryBytes: number,
    readonly ci: string | null,
    readonly loadBefore: string,
    readonly loadAfter: string,
  |},
  readonly versions: $ReadOnlyArray<[string, string]>,
  readonly runs: number,
  readonly warmup: number,
  readonly arguments: string,
  readonly fixtures: $ReadOnlyArray<[string, string]>,
  readonly skipped: $ReadOnlyArray<Skipped>,
  readonly rows: $ReadOnlyArray<Row>,
|};

function fail(what: string): empty {
  throw new Error(
    `tools/bench/toolchain/baseline.json ${what}; it is not a file bench.js wrote, ` +
      "and the benchmarks page will not render a guess",
  );
}

function object(value: mixed, what: string): { readonly [string]: mixed } {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return fail(`has no object for ${what}`);
  }
  return value;
}

function text(value: mixed, what: string): string {
  return typeof value === "string" ? value : fail(`has no string for ${what}`);
}

function number(value: mixed, what: string): number {
  return typeof value === "number" ? value : fail(`has no number for ${what}`);
}

function array(value: mixed, what: string): $ReadOnlyArray<mixed> {
  return Array.isArray(value) ? value : fail(`has no array for ${what}`);
}

function load(value: mixed): string {
  const loads = object(value, "machine load");
  return array(loads.load, "machine load")
    .map((one) => String(number(one, "a load average")))
    .join(" ");
}

function describeFixture(name: string, value: mixed): string {
  const fixture = object(value, `fixture ${name}`);
  const count = (key: string) => String(number(fixture[key], `fixture ${name} ${key}`));
  return (
    `${count("routes")} routes, ${count("components")} components ` +
    `(${count("clientComponents")} client), ${count("modules")} library modules, ` +
    `${count("tests")} tests in ${count("testFiles")} files`
  );
}

function parse(source: string): Run {
  const top = object(JSON.parse(source), "the run");
  const machine = object(top.machine, "machine");
  const settings = object(top.settings, "settings");
  const versions = object(top.versions, "versions");
  const fixtures: Array<[string, string]> = Object.keys(object(top.fixtures, "fixtures")).map(
    (name) => [name, describeFixture(name, object(top.fixtures, "fixtures")[name])],
  );
  if (top.suite != null) {
    const suite = object(top.suite, "suite");
    fixtures.push([
      "suite",
      `${String(number(suite.tests, "suite tests"))} tests in ` +
        `${String(number(suite.files, "suite files"))} files, ` +
        `${String(number(suite.assertions, "suite assertions"))} assertions`,
    ]);
  }
  if (top.install != null) {
    const install = object(top.install, "install");
    fixtures.push([
      "install",
      `${String(number(install.dependencies, "install dependencies"))} direct dependencies ` +
        `of a scaffolded application, pinned; \`uf install\` ran ${text(install.manager, "install manager")}`,
    ]);
  }
  return {
    finishedAt: text(top.finishedAt, "finishedAt"),
    provisional: top.provisional === true,
    machine: {
      platform: text(machine.platform, "machine platform"),
      arch: text(machine.arch, "machine arch"),
      cpu: text(machine.cpu, "machine cpu"),
      cores: number(machine.cores, "machine cores"),
      memoryBytes: number(machine.memoryBytes, "machine memory"),
      ci: machine.ci == null ? null : text(machine.ci, "machine ci"),
      loadBefore: load(machine.before),
      loadAfter: load(machine.after),
    },
    versions: Object.keys(versions).map((name) => [name, text(versions[name], `version ${name}`)]),
    runs: number(settings.runs, "settings runs"),
    warmup: number(settings.warmup, "settings warmup"),
    arguments: array(top.arguments, "arguments")
      .map((one) => text(one, "an argument"))
      .join(" "),
    fixtures,
    skipped: array(top.skipped ?? [], "skipped").map((value) => {
      const one = object(value, "a skipped tool");
      return {
        tool: text(one.tool, "skipped tool"),
        stage: one.stage == null ? null : text(one.stage, "skipped stage"),
        reason: text(one.reason, "skipped reason"),
      };
    }),
    rows: array(top.results, "results").map((value) => {
      const row = object(value, "a result");
      const cpu = row.cpu == null ? null : object(row.cpu, "a result's cpu");
      return {
        tool: text(row.tool, "a result's tool"),
        stage: text(row.stage, "a result's stage"),
        fixture: text(row.fixture, "a result's fixture"),
        title: text(row.title, "a result's title"),
        command: text(row.command, "a result's command"),
        cache: text(row.cache, "a result's cache"),
        runs: number(row.runs, "a result's runs"),
        median: number(row.median, "a result's median"),
        min: number(row.min, "a result's min"),
        max: number(row.max, "a result's max"),
        cpu: cpu == null ? null : { median: number(cpu.median, "a result's cpu median") },
      };
    }),
  };
}

/** The committed run, or null before one has been committed. */
function committed(): Run | null {
  const sources = Object.keys(FILES).map((key) => FILES[key]);
  return sources.length === 0 ? null : parse(sources[0]);
}

const RUN = committed();

/** Milliseconds as a person reads them, `report.js`'s way: `8.4 ms`, `412 ms`, `3.01 s`. */
function duration(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(2)} s`;
  }
  if (ms >= 10) {
    return `${String(Math.round(ms))} ms`;
  }
  return `${ms.toFixed(1)} ms`;
}

type Group = {| readonly key: string, readonly title: string, readonly rows: $ReadOnlyArray<Row> |};

/** The rows by fixture and stage, in the order the run measured them, fastest first. */
function groups(rows: $ReadOnlyArray<Row>): $ReadOnlyArray<Group> {
  const found: Map<string, Array<Row>> = new Map();
  for (const row of rows) {
    const key = `${row.fixture} ${row.stage}`;
    const group = found.get(key);
    if (group == null) {
      found.set(key, [row]);
    } else {
      group.push(row);
    }
  }
  return [...found.entries()].map(([key, members]) => ({
    key,
    title: `${members[0].title} — ${members[0].fixture}`,
    rows: [...members].sort((a, b) => a.median - b.median),
  }));
}

type Loss = {| readonly group: Group, readonly uf: Row, readonly faster: $ReadOnlyArray<Row> |};

/** Every group in which another tool's median is below uf's. */
function losses(run: Run): $ReadOnlyArray<Loss> {
  const found: Array<Loss> = [];
  for (const group of groups(run.rows)) {
    const uf = group.rows.find((row) => row.tool === "uf");
    if (uf == null) {
      continue;
    }
    const faster = group.rows.filter((row) => row.tool !== "uf" && row.median < uf.median);
    if (faster.length > 0) {
      found.push({ group, uf, faster });
    }
  }
  return found;
}

function ratio(row: Row, uf: Row | void): string {
  if (uf == null || row.tool === "uf") {
    return "";
  }
  const times = row.median / uf.median;
  return times >= 1 ? `${times.toFixed(1)}× uf` : `${(1 / times).toFixed(1)}× faster than uf`;
}

/** Said when nothing has been committed yet, rather than a table of nothing. */
component NotYet() {
  return (
    <p>
      No run has been committed yet. The first is the <code>bench-comparison</code> artifact of the
      Bench workflow, committed as <code>tools/bench/toolchain/baseline.json</code>.
    </p>
  );
}

/** Where the numbers came from: the machine, the load, the run and every version. */
export component Provenance() {
  const run = RUN;
  if (run == null) {
    return <NotYet />;
  }
  const { machine } = run;
  return (
    <>
      <table>
        <tbody>
          <tr>
            <th scope="row">Finished</th>
            <td>{run.finishedAt}</td>
          </tr>
          <tr>
            <th scope="row">Machine</th>
            <td>
              {machine.cpu}, {machine.cores} cores, {(machine.memoryBytes / 1024 ** 3).toFixed(1)}{" "}
              GB, {machine.platform} {machine.arch}
            </td>
          </tr>
          <tr>
            <th scope="row">Runner</th>
            <td>{machine.ci ?? "not CI: a person's machine"}</td>
          </tr>
          <tr>
            <th scope="row">Load average</th>
            <td>
              {machine.loadBefore} before, {machine.loadAfter} after
              {run.provisional ? " — not quiet when the run started, so provisional" : ""}
            </td>
          </tr>
          <tr>
            <th scope="row">Runs</th>
            <td>
              {run.runs} timed runs of every stage after {run.warmup} thrown away; the median is
              shown
            </td>
          </tr>
          <tr>
            <th scope="row">Command</th>
            <td>
              <code>uf run bench:toolchain -- {run.arguments}</code>
            </td>
          </tr>
          {run.fixtures.map(([name, description]) => (
            <tr key={name}>
              <th scope="row">
                Fixture <code>{name}</code>
              </th>
              <td>{description}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <table>
        <thead>
          <tr>
            <th>Tool or input</th>
            <th>Version</th>
          </tr>
        </thead>
        <tbody>
          {run.versions.map(([name, version]) => (
            <tr key={name}>
              <td>{name}</td>
              <td>
                <code>{version}</code>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

/** The rows uf loses: every stage where another tool's median is lower. */
export component Losses() {
  const run = RUN;
  if (run == null) {
    return <NotYet />;
  }
  const lost = losses(run);
  if (lost.length === 0) {
    return <p>In this run, uf has the lowest median in every stage it was compared in.</p>;
  }
  return (
    <table>
      <thead>
        <tr>
          <th>Stage</th>
          <th>uf</th>
          <th>Faster</th>
        </tr>
      </thead>
      <tbody>
        {lost.map(({ group, uf, faster }) => (
          <tr key={group.key}>
            <td>{group.title}</td>
            <td>
              <code>{uf.command}</code> {duration(uf.median)}
            </td>
            <td>
              {faster.map((row, index) => (
                <React.Fragment key={row.tool}>
                  {index > 0 ? "; " : ""}
                  <code>{row.command}</code> {duration(row.median)}
                </React.Fragment>
              ))}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Every row, a table per stage, fastest first, with its command and its spread. */
export component Results() {
  const run = RUN;
  if (run == null) {
    return <NotYet />;
  }
  const versions = new Map(run.versions);
  return (
    <>
      {groups(run.rows).map((group) => {
        const uf = group.rows.find((row) => row.tool === "uf");
        return (
          <section key={group.key}>
            <h3>{group.title}</h3>
            <table>
              <thead>
                <tr>
                  <th>Command</th>
                  <th>Version</th>
                  <th>Median</th>
                  <th>Min–max</th>
                  <th>CPU</th>
                  <th>Against uf</th>
                  <th>Between runs</th>
                </tr>
              </thead>
              <tbody>
                {group.rows.map((row) => (
                  <tr key={row.tool}>
                    <td>
                      <code>{row.command}</code>
                    </td>
                    <td>{versions.get(row.tool === "uf" ? "uf" : row.tool) ?? "unknown"}</td>
                    <td>{duration(row.median)}</td>
                    <td>
                      {duration(row.min)}–{duration(row.max)}
                    </td>
                    <td>{row.cpu == null ? "—" : duration(row.cpu.median)}</td>
                    <td>{ratio(row, uf)}</td>
                    <td>{row.cache}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        );
      })}
      {run.skipped.length === 0 ? null : (
        <>
          <h3>Asked for and not measured</h3>
          <ul>
            {run.skipped.map((skipped) => (
              <li key={`${skipped.tool} ${skipped.stage ?? ""}`}>
                <code>{skipped.tool}</code>
                {skipped.stage == null ? "" : ` ${skipped.stage}`}: {skipped.reason}
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}

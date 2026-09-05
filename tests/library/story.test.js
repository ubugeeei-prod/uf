// @flow
//
// `@uniflowed/story`.
//
// Four things have to be true for a story to be worth declaring, and each has
// a section below: it can be declared and found, it can be rendered and
// asserted on, the setup it declares actually reaches it, and a play function
// fails when the component is broken.
//
// The collection tests write real `_uf.story.js` files into a temporary
// directory and import them. A fixture built by calling `defineStories`
// in-process would prove the walk finds a path and nothing about whether the
// file at the end of it loads — which is the half that breaks.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { HttpResponse, http } from "@uniflowed/mock";
import * as React from "@uniflowed/react";
import { useEffect, useState } from "@uniflowed/react";
import { screen } from "@uniflowed/react-testing";
import { describe, expect, it } from "@uniflowed/test";
import {
  STORY_FILE,
  StoryPlayError,
  classifyStoryFile,
  collectStories,
  defineStories,
  findStory,
  findStoryFiles,
  indexStories,
  isStorySet,
  isStoryEntry,
  loadStoryFile,
  mountStory,
  renderStoryToHtml,
  storyId,
} from "@uniflowed/story";
import { describeStories, storyTest } from "@uniflowed/story/runner";

const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

// --- Components the stories below describe -----------------------------

component Badge(label: string, tone: string) {
  return <span data-tone={tone}>{label}</span>;
}

component Counter(step: number) {
  const [count, setCount] = useState(0);
  return (
    <div>
      <output role="status">{count}</output>
      <button type="button" onClick={() => setCount(count + step)}>
        add
      </button>
    </div>
  );
}

/** `Counter` with the bug this suite exists to catch: `step` is ignored. */
component StuckCounter(step: number) {
  const [count, setCount] = useState(0);
  return (
    <div>
      <output role="status">{count}</output>
      <button type="button" onClick={() => setCount(count)}>
        add
      </button>
    </div>
  );
}

component Profile(id: string) {
  const [name, setName] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    fetch(`/users/${id}`)
      .then((response) => response.json())
      .then((body) => {
        if (live) {
          setName(String(body.name));
        }
      })
      .catch(() => {
        if (live) {
          setFailed(true);
        }
      });
    return () => {
      live = false;
    };
  }, [id]);

  if (failed) {
    return <p role="alert">could not load</p>;
  }
  return <p>{name ?? "loading"}</p>;
}

component Exploding() {
  throw new Error("this component is broken");
}

// --- Fixtures ----------------------------------------------------------

const badges = defineStories({
  title: "Badge",
  component: Badge,
  props: { label: "Ready", tone: "neutral" },
  stories: {
    Neutral: {},
    Warning: { name: "Needs attention", props: { tone: "warning" } },
  },
});

/**
 * A directory to build a story tree in.
 *
 * The workspace's `node_modules` is linked in so a fixture can
 * `import "@uniflowed/story"` the way a project would — Node resolves by
 * walking up from the importing file, so the link sits one level *above* the
 * directory the tests write into. That is deliberate: a fixture that wrote
 * `node_modules/…` under the walk root would create the directory through
 * the link and leave it in this repository's real `node_modules`, which is
 * exactly what an earlier draft of this file did.
 */
function temporaryProject(): string {
  const enclosing = fs.mkdtempSync(path.join(os.tmpdir(), "uf-story-"));
  fs.symlinkSync(
    path.join(repository, "node_modules"),
    path.join(enclosing, "node_modules"),
    "dir",
  );
  const root = path.join(enclosing, "project");
  fs.mkdirSync(root);
  return root;
}

/** Write `contents` at `relative` under `root`, creating directories. */
function write(root: string, relative: string, contents: string): string {
  const file = path.join(root, relative);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents);
  return file;
}

/** A story file that declares `count` stories of a component called `title`. */
function storyFile(title: string, names: $ReadOnlyArray<string>): string {
  const stories = names.map((name) => `      ${name}: {},`).join("\n");
  return [
    "// @flow",
    'import { defineStories } from "@uniflowed/story";',
    "",
    `component Subject(label: string) {`,
    "  return <p>{label}</p>;",
    "}",
    "",
    "export const stories = defineStories({",
    `  title: ${JSON.stringify(title)},`,
    "  component: Subject,",
    `  props: { label: ${JSON.stringify(title)} },`,
    "  stories: {",
    stories,
    "  },",
    "});",
    "",
  ].join("\n");
}

/** Run `body` with a temporary project, removed however it ends. */
async function withProject(body: (root: string) => Promise<void>): Promise<void> {
  const root = temporaryProject();
  try {
    await body(root);
  } finally {
    // The enclosing directory, so the `node_modules` link goes with it. `rm`
    // removes a link rather than what it points at, so this cannot reach the
    // repository.
    fs.rmSync(path.dirname(root), { recursive: true, force: true });
  }
}

// --- Declaring ---------------------------------------------------------

describe("defineStories", () => {
  it("names a story after the key it was declared under", () => {
    expect(badges.stories.map((story) => story.name)).toEqual(["Neutral", "Needs attention"]);
    expect(badges.stories.map((story) => story.key)).toEqual(["Neutral", "Warning"]);
  });

  it("gives every story a slugged id built from the title and the name", () => {
    expect(badges.stories.map((story) => story.id)).toEqual([
      "badge--neutral",
      "badge--needs-attention",
    ]);
    expect(storyId("Forms/Text Field", "Empty")).toBe("forms-text-field--empty");
  });

  it("applies a story's props over the set's", () => {
    expect(badges.stories[0].props).toEqual({ label: "Ready", tone: "neutral" });
    expect(badges.stories[1].props).toEqual({ label: "Ready", tone: "warning" });
  });

  it("gives each story its own props object", () => {
    expect(badges.stories[0].props).not.toBe(badges.stories[1].props);
  });

  it("keeps the set's decorators outside the story's", () => {
    const order = [];
    const set = defineStories({
      title: "Badge",
      component: Badge,
      props: { label: "Ready", tone: "neutral" },
      decorators: [
        (children) => {
          order.push("set");
          return children;
        },
      ],
      stories: {
        Only: {
          decorators: [
            (children) => {
              order.push("story");
              return children;
            },
          ],
        },
      },
    });

    // Applied innermost first, so the set's runs last and ends up outermost.
    mountStory(set.stories[0]).unmount();
    expect(order).toEqual(["story", "set"]);
  });

  it("offers a story's handlers before the set's", () => {
    const setHandler = http.get("/a", () => HttpResponse.json({}));
    const storyHandler = http.get("/b", () => HttpResponse.json({}));
    const set = defineStories({
      title: "Badge",
      component: Badge,
      props: { label: "Ready", tone: "neutral" },
      mocks: [setHandler],
      stories: { Only: { mocks: [storyHandler] } },
    });

    expect(set.stories[0].mocks).toEqual([storyHandler, setHandler]);
  });

  it("falls back to the set's play function", () => {
    const set = defineStories({
      title: "Badge",
      component: Badge,
      props: { label: "Ready", tone: "neutral" },
      play: () => {},
      stories: { Inherits: {}, Overrides: { play: () => {} } },
    });

    expect(set.stories[0].play).not.toBe(null);
    expect(set.stories[1].play).not.toBe(set.stories[0].play);
  });

  it("refuses a set with no stories", () => {
    expect(() =>
      defineStories({
        title: "Badge",
        component: Badge,
        props: { label: "Ready", tone: "neutral" },
        stories: {},
      }),
    ).toThrow("declares no stories");
  });

  it("recognises its own sets and nothing else", () => {
    expect(isStorySet(badges)).toBe(true);
    expect(isStorySet({ kind: "uniflowed/story-set" })).toBe(false);
    expect(isStorySet(null)).toBe(false);
    expect(isStorySet("badge")).toBe(false);
  });
});

describe("findStory", () => {
  it("finds a story by its key or by its name", () => {
    expect(findStory(badges, "Warning").id).toBe("badge--needs-attention");
    expect(findStory(badges, "Needs attention").id).toBe("badge--needs-attention");
  });

  it("says what the set does hold when it does not hold that", () => {
    expect(() => findStory(badges, "Danger")).toThrow("Neutral, Warning");
  });
});

// --- Collecting --------------------------------------------------------

describe("the reserved story file name", () => {
  it("accepts the name and the router's variants", () => {
    expect(STORY_FILE).toBe("_uf.story.js");
    expect(classifyStoryFile(STORY_FILE)).toBe("default");
    expect(classifyStoryFile("_uf.story.native.js")).toBe("native");
    expect(classifyStoryFile("_uf.story.ios.js")).toBe("ios");
    expect(classifyStoryFile("_uf.story.android.js")).toBe("android");
    expect(classifyStoryFile("_uf.story.web.js")).toBe("web");
    expect(classifyStoryFile("_uf.story.test.js")).toBe("test");
  });

  it("rejects names uf does not define", () => {
    for (const name of [
      "_uf.stories.js",
      "_uf.story.server.js",
      "_uf.story.native.test.js",
      "_uf.story.jsx",
      "_uf.story",
      "_uf.STORY.js",
      "_uf.page.js",
    ]) {
      expect(classifyStoryFile(name)).toBe(null);
    }
  });

  it("leaves project-owned names alone", () => {
    for (const name of ["Button.stories.js", "story.js", "_private.js", "index.js"]) {
      expect(classifyStoryFile(name)).toBe(null);
    }
  });

  it("treats only the default variant as the file a renderer mounts", () => {
    expect(isStoryEntry("_uf.story.js")).toBe(true);
    expect(isStoryEntry("_uf.story.native.js")).toBe(false);
  });
});

describe("findStoryFiles", () => {
  it("walks a tree and returns story files in a stable order", async () => {
    await withProject(async (root) => {
      write(root, "src/z/_uf.story.js", storyFile("Z", ["Only"]));
      write(root, "src/a/_uf.story.js", storyFile("A", ["Only"]));
      write(root, "src/a/Component.js", "// @flow\n");

      const found = await findStoryFiles(root);

      expect(found.map((file) => path.relative(root, file))).toEqual([
        path.join("src", "a", "_uf.story.js"),
        path.join("src", "z", "_uf.story.js"),
      ]);
    });
  });

  it("leaves ignored directories, the variants and symlinks alone", async () => {
    await withProject(async (root) => {
      write(root, "src/_uf.story.js", storyFile("Kept", ["Only"]));
      write(root, "src/_uf.story.native.js", storyFile("Native", ["Only"]));
      write(root, "node_modules/other/_uf.story.js", storyFile("Vendored", ["Only"]));
      write(root, "dist/_uf.story.js", storyFile("Built", ["Only"]));
      // A link to a directory that does hold a story file, so following it
      // would report the same story twice under two paths.
      fs.symlinkSync(path.join(root, "src"), path.join(root, "linked"), "dir");

      const found = await findStoryFiles(root);

      expect(found.map((file) => path.relative(root, file))).toEqual([
        path.join("src", "_uf.story.js"),
      ]);
    });
  });

  it("stops at the depth it was given", async () => {
    await withProject(async (root) => {
      write(root, "a/_uf.story.js", storyFile("Shallow", ["Only"]));
      write(root, "a/b/c/_uf.story.js", storyFile("Deep", ["Only"]));

      const found = await findStoryFiles(root, { maxDepth: 1 });

      expect(found.map((file) => path.relative(root, file))).toEqual([
        path.join("a", "_uf.story.js"),
      ]);
    });
  });
});

describe("loadStoryFile", () => {
  it("returns every set a file exports", async () => {
    await withProject(async (root) => {
      const file = write(
        root,
        "_uf.story.js",
        [
          "// @flow",
          'import { defineStories } from "@uniflowed/story";',
          "",
          "component Subject(label: string) {",
          "  return <p>{label}</p>;",
          "}",
          "",
          "const shape = { component: Subject, props: { label: 'x' }, stories: { Only: {} } };",
          "export const first = defineStories({ title: 'First', ...shape });",
          "export const second = defineStories({ title: 'Second', ...shape });",
          "export const notASet = { title: 'Third' };",
          "",
        ].join("\n"),
      );

      const sets = await loadStoryFile(file);

      expect(sets.map((set) => set.title)).toEqual(["First", "Second"]);
    });
  });

  it("refuses a story file that exports no set", async () => {
    await withProject(async (root) => {
      const file = write(root, "_uf.story.js", "// @flow\nexport const nothing: number = 1;\n");

      await expect(loadStoryFile(file)).rejects.toThrow("exports no story set");
    });
  });
});

describe("collectStories", () => {
  it("indexes every story a project declares", async () => {
    await withProject(async (root) => {
      write(root, "src/badge/_uf.story.js", storyFile("Badge", ["Neutral", "Warning"]));
      write(root, "src/card/_uf.story.js", storyFile("Card", ["Only"]));

      const index = await collectStories(root);

      expect(index.files.length).toBe(2);
      expect(index.sets.map((set) => set.title)).toEqual(["Badge", "Card"]);
      expect(index.stories.map((story) => story.id)).toEqual([
        "badge--neutral",
        "badge--warning",
        "card--only",
      ]);
      expect(index.get("card--only")?.title).toBe("Card");
      expect(index.get("card--missing")).toBe(undefined);
    });
  });

  it("renders a story it collected without knowing where it came from", async () => {
    await withProject(async (root) => {
      write(root, "src/_uf.story.js", storyFile("Collected", ["Only"]));

      const index = await collectStories(root);
      const story = index.get("collected--only");
      if (story == null) {
        throw new Error("the story was not collected");
      }

      expect(await renderStoryToHtml(story)).toBe("<p>Collected</p>");
    });
  });

  it("refuses two stories that share an id", async () => {
    await withProject(async (root) => {
      write(root, "one/_uf.story.js", storyFile("Badge", ["Only"]));
      write(root, "two/_uf.story.js", storyFile("Badge", ["Only"]));

      const files = await findStoryFiles(root);

      await expect(indexStories(files)).rejects.toThrow("two stories share the id badge--only");
    });
  });
});

// --- Rendering ---------------------------------------------------------

describe("mountStory", () => {
  it("renders the story's props", () => {
    const mounted = mountStory(findStory(badges, "Warning"));
    try {
      const badge = mounted.canvas.getByText("Ready");
      expect(badge.getAttribute("data-tone")).toBe("warning");
    } finally {
      mounted.unmount();
    }
  });

  it("scopes the canvas to the story", () => {
    const mounted = mountStory(findStory(badges, "Neutral"));
    try {
      expect(mounted.canvas.queryAllByText("Ready").length).toBe(1);
      expect(mounted.container.contains(mounted.canvas.getByText("Ready"))).toBe(true);
    } finally {
      mounted.unmount();
    }
  });

  it("takes the story down again", () => {
    const mounted = mountStory(findStory(badges, "Neutral"));
    mounted.unmount();
    // Twice, because a play function that threw leaves a caller unmounting in
    // a `finally` after the runner has already done it.
    mounted.unmount();

    expect(screen.queryAllByText("Ready").length).toBe(0);
  });

  it("wraps the story in its decorators, first outermost", async () => {
    const set = defineStories({
      title: "Badge",
      component: Badge,
      props: { label: "Ready", tone: "neutral" },
      decorators: [
        (children) => <section data-frame="outer">{children}</section>,
        (children) => <div data-frame="inner">{children}</div>,
      ],
      stories: { Only: {} },
    });

    expect(await renderStoryToHtml(set.stories[0])).toBe(
      '<section data-frame="outer"><div data-frame="inner">' +
        '<span data-tone="neutral">Ready</span></div></section>',
    );
  });

  it("serialises a story without a caller mounting React", async () => {
    expect(await renderStoryToHtml(findStory(badges, "Neutral"))).toBe(
      '<span data-tone="neutral">Ready</span>',
    );
  });
});

// --- Mocked requests ---------------------------------------------------

describe("a story's mocked requests", () => {
  const profiles = defineStories({
    title: "Profile",
    component: Profile,
    props: { id: "42" },
    mocks: [
      http.get("/users/:id", ({ params }) => HttpResponse.json({ name: `user ${params.id}` })),
    ],
    stories: {
      Loaded: {},
      Missing: {
        // Overrides the set's handler, because a story's own is offered first.
        mocks: [http.get("/users/:id", () => new HttpResponse(null, { status: 404 }))],
      },
    },
  });

  it("answers the request the story's component makes", async () => {
    const mounted = mountStory(findStory(profiles, "Loaded"));
    try {
      expect(await mounted.canvas.findByText("user 42")).toBeTruthy();
    } finally {
      mounted.unmount();
    }
  });

  it("records what the story asked for", async () => {
    const mounted = mountStory(findStory(profiles, "Loaded"));
    try {
      await mounted.canvas.findByText("user 42");
      expect(mounted.requests.length).toBe(1);
      expect(mounted.requests[0].method).toBe("GET");
      expect(mounted.requests[0].pathname).toBe("/users/42");
    } finally {
      mounted.unmount();
    }
  });

  it("lets a story's own handler win over the set's", async () => {
    const mounted = mountStory(findStory(profiles, "Missing"));
    try {
      // A 404 body is not JSON, so the component's `catch` runs.
      expect(await mounted.canvas.findByRole("alert")).toBeTruthy();
    } finally {
      mounted.unmount();
    }
  });

  it("refuses a request the story did not declare", async () => {
    const set = defineStories({
      title: "Profile",
      component: Profile,
      props: { id: "7" },
      mocks: [http.get("/health", () => HttpResponse.json({ ok: true }))],
      stories: { Loaded: {} },
    });

    const mounted = mountStory(set.stories[0]);
    try {
      // Nothing answers `/users/7`, so `fetch` rejects and the component's
      // own `catch` puts the alert on screen. The request is in the log
      // either way, which is how a story's author sees what it asked for.
      expect(await mounted.canvas.findByRole("alert")).toBeTruthy();
      expect(mounted.requests.map((request) => request.pathname)).toEqual(["/users/7"]);
    } finally {
      mounted.unmount();
    }
  });

  it("stops intercepting when the story comes down", async () => {
    const mounted = mountStory(findStory(profiles, "Loaded"));
    await mounted.canvas.findByText("user 42");
    mounted.unmount();

    // A second mount is the assertion: `listen()` refuses to nest, so this
    // throws if the first registry was left installed.
    const again = mountStory(findStory(profiles, "Loaded"));
    try {
      expect(await again.canvas.findByText("user 42")).toBeTruthy();
    } finally {
      again.unmount();
    }
  });

  // uf-lint-disable fetch/no-global-override
  //
  // The rule is right about application code and wrong about these two tests.
  // Whether a registry was installed, and whether it was taken back out, is a
  // fact about `globalThis.fetch` and there is no other way to observe it —
  // the alternative is a test that asserts the interception worked by using
  // the interception.

  it("does not intercept for a story that declares no handlers", () => {
    const before = globalThis.fetch;
    const mounted = mountStory(findStory(badges, "Neutral"));
    try {
      expect(globalThis.fetch).toBe(before);
      expect(mounted.requests.length).toBe(0);
    } finally {
      mounted.unmount();
    }
  });

  it("puts fetch back when the story throws while mounting", () => {
    const before = globalThis.fetch;
    const set = defineStories({
      title: "Exploding",
      component: Exploding,
      props: {},
      mocks: [http.get("/anything", () => HttpResponse.json({}))],
      stories: { Only: {} },
    });

    expect(() => mountStory(set.stories[0])).toThrow("this component is broken");
    expect(globalThis.fetch).toBe(before);
  });

  // uf-lint-enable fetch/no-global-override
});

// --- Play functions ----------------------------------------------------

describe("a play function", () => {
  /** `Counter` stories, over whichever counter is handed in. */
  function counters(component) {
    return defineStories({
      title: "Counter",
      component,
      props: { step: 2 },
      stories: {
        Added: {
          play: async ({ canvas, user, step }) => {
            await step("presses add", async () => {
              await user.click(canvas.getByRole("button", { name: "add" }));
            });
            const shown = canvas.getByRole("status").textContent;
            if (shown !== "2") {
              throw new Error(`the counter shows ${String(shown)}`);
            }
          },
        },
      },
    });
  }

  it("drives the story and passes when the component works", async () => {
    const mounted = mountStory(counters(Counter).stories[0]);
    try {
      await mounted.play();
      expect(mounted.canvas.getByRole("status").textContent).toBe("2");
    } finally {
      mounted.unmount();
    }
  });

  it("fails when the component is broken", async () => {
    const mounted = mountStory(counters(StuckCounter).stories[0]);
    try {
      await expect(mounted.play()).rejects.toThrow("the counter shows 0");
    } finally {
      mounted.unmount();
    }
  });

  it("names the story and the step a failure happened in", async () => {
    const set = defineStories({
      title: "Counter",
      component: Counter,
      props: { step: 2 },
      stories: {
        Added: {
          play: async ({ step }) => {
            await step("presses add", () => {
              throw new Error("nothing to press");
            });
          },
        },
      },
    });

    const mounted = mountStory(set.stories[0]);
    try {
      await mounted.play();
      throw new Error("the play function should have failed");
    } catch (error) {
      expect(error instanceof StoryPlayError).toBe(true);
      expect(error.message).toBe("counter--added > presses add: nothing to press");
      expect(error.story).toBe("counter--added");
      expect(error.step).toBe("presses add");
      expect(error.cause instanceof Error).toBe(true);
    } finally {
      mounted.unmount();
    }
  });

  it("reports a failure outside every step with no step name", async () => {
    const set = defineStories({
      title: "Counter",
      component: Counter,
      props: { step: 2 },
      stories: {
        Added: {
          play: () => {
            throw new Error("straight out");
          },
        },
      },
    });

    const mounted = mountStory(set.stories[0]);
    try {
      await expect(mounted.play()).rejects.toThrow("counter--added: straight out");
    } finally {
      mounted.unmount();
    }
  });

  it("hands the play function the props the story was rendered with", async () => {
    let seen = null;
    const set = defineStories({
      title: "Badge",
      component: Badge,
      props: { label: "Ready", tone: "neutral" },
      stories: {
        Warning: {
          props: { tone: "warning" },
          play: ({ props }) => {
            seen = props.tone;
          },
        },
      },
    });

    const mounted = mountStory(set.stories[0]);
    try {
      await mounted.play();
      expect(seen).toBe("warning");
    } finally {
      mounted.unmount();
    }
  });

  it("can be run before the markup is serialised", async () => {
    const set = defineStories({
      title: "Counter",
      component: Counter,
      props: { step: 3 },
      stories: {
        Added: {
          play: async ({ canvas, user }) => {
            await user.click(canvas.getByRole("button", { name: "add" }));
          },
        },
      },
    });

    expect(await renderStoryToHtml(set.stories[0])).toContain(">0<");
    expect(await renderStoryToHtml(set.stories[0], { play: true })).toContain(">3<");
  });

  it("resolves for a story that has none", async () => {
    const mounted = mountStory(findStory(badges, "Neutral"));
    try {
      await mounted.play();
    } finally {
      mounted.unmount();
    }
  });
});

// --- The bridge to `uf test` -------------------------------------------

describe("storyTest", () => {
  it("mounts, plays and unmounts one story", async () => {
    await storyTest(findStory(badges, "Neutral"))();

    expect(screen.queryAllByText("Ready").length).toBe(0);
  });

  it("unmounts a story whose play function failed", async () => {
    const set = defineStories({
      title: "Badge",
      component: Badge,
      props: { label: "Ready", tone: "neutral" },
      stories: {
        Only: {
          play: () => {
            throw new Error("no");
          },
        },
      },
    });

    await expect(storyTest(set.stories[0])()).rejects.toThrow("badge--only: no");
    expect(screen.queryAllByText("Ready").length).toBe(0);
  });
});

// One test per story, registered from the set rather than written out. The
// names below are not string literals, so `uf test` cannot discover them —
// the literal declarations above are what make this file run at all, and
// `packages/story/runner.js` says so at length.
describeStories(badges);

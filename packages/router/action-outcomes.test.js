// @flow
//
// `redirect()`, `notFound()`, `unauthorized()` and `forbidden()` thrown from a
// server action, through both doors and back into the browser.
//
// They are how page code says where the visitor goes next, and an action is
// page code. Before ubugeeei-prod/uf#1470 only the door a form posts through
// before hydration knew them; the JSON door a hydrated page calls reported
// each as a failure and answered `500`, so one form did two different things
// depending on whether its JavaScript had arrived. What is held here is that
// both doors answer every one of them as itself, that nothing of the exception
// crosses either way, and that the reference in the browser does what the
// no-JavaScript path does.

import * as React from "@uniflowed/react";
import { act, cleanup, render, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { ServerActionError, createServerReference } from "./action.js";
import { ACTION_OUTCOME_HEADER } from "./internal/action-wire.js";
import { FORM_ACTION_CONTENT_TYPE } from "./internal/form-action.js";
import {
  ForbiddenError,
  NotFoundError,
  UnauthorizedError,
  forbidden,
  notFound,
  redirect,
  unauthorized,
} from "./internal/routing.js";
import { installNavigation, installRoutes, resolveMatch, routerView } from "./internal/runtime.js";
import { beginRequest, createActionDispatcher } from "./server.js";

const globals: $FlowFixMe = globalThis;
const ID = "a1b2c3d4e5f6".repeat(6).slice(0, 64);

afterEach(() => {
  if (globals.document != null) {
    cleanup();
    globals.document.body.replaceChildren();
    globals.window.history.replaceState(null, "", "/");
  }
  uft.restoreAllMocks();
  installNavigation("client");
});

/** Run `action` behind a dispatcher, inside a request, as a host does. */
async function dispatched(action: () => Promise<mixed>, request: Request): Promise<Response> {
  const callAction = createActionDispatcher({
    actions: [
      {
        id: ID,
        module: "app/notes/_actions.js",
        export: "save",
        load: async () => ({ save: action }),
      },
    ],
  });
  const { run, settle } = beginRequest(request);
  try {
    const answer = await run(() => callAction(request));
    if (answer == null) {
      throw new Error("the dispatcher declined an action call");
    }
    return answer;
  } finally {
    await settle();
  }
}

const SAME_ORIGIN = { origin: "https://app.example", host: "app.example" };

/** A hydrated page's call: JSON, with the id in the header. */
const jsonCall = () =>
  new Request("https://app.example/notes", {
    method: "POST",
    headers: { ...SAME_ORIGIN, "content-type": "application/json", "uf-action": ID },
    body: '{"args":[]}',
  });

/** A form posted before its page hydrated. */
const formPost = () =>
  new Request("https://app.example/notes", {
    method: "POST",
    headers: {
      ...SAME_ORIGIN,
      "content-type": FORM_ACTION_CONTENT_TYPE,
      "sec-fetch-site": "same-origin",
    },
    body: new URLSearchParams([
      ["$uf_ref_R0", ""],
      ["$uf_id_R0", ID],
    ]).toString(),
  });

/** Run `body` with `console.error` recorded, and hand back what it was given. */
async function reportedDuring(body: () => Promise<void>): Promise<Array<mixed>> {
  const reported: Array<mixed> = [];
  uft.spyOn(console, "error").mockImplementation((...args: Array<mixed>) => {
    reported.push(args);
  });
  await body();
  return reported;
}

describe("a routing call in an action, through the JSON door", () => {
  it("answers redirect() with where it points, and reports nothing", async () => {
    let answer: Response | null = null;
    const reported = await reportedDuring(async () => {
      answer = await dispatched(async () => redirect("/notes/7"), jsonCall());
    });
    expect(answer?.status).toBe(204);
    expect(answer?.headers.get(ACTION_OUTCOME_HEADER)).toBe("redirect");
    expect(answer?.headers.get("location")).toBe("/notes/7");
    expect(reported).toEqual([]);
  });

  it("answers notFound(), unauthorized() and forbidden() with their page statuses", async () => {
    const cases: $ReadOnlyArray<[() => Promise<mixed>, number, string]> = [
      [async () => notFound(), 404, "not-found"],
      [async () => unauthorized(), 401, "unauthorized"],
      [async () => forbidden(), 403, "forbidden"],
    ];
    for (const [action, status, kind] of cases) {
      let answer: Response | null = null;
      const reported = await reportedDuring(async () => {
        answer = await dispatched(action, jsonCall());
      });
      expect(answer?.status).toBe(status);
      expect(answer?.headers.get(ACTION_OUTCOME_HEADER)).toBe(kind);
      // The kind, and nothing of the exception.
      expect(await answer?.text()).toBe(JSON.stringify({ outcome: kind }));
      expect(reported).toEqual([]);
    }
  });

  it("still answers any other throw with the fixed 500, and no outcome", async () => {
    let answer: Response | null = null;
    await reportedDuring(async () => {
      answer = await dispatched(async () => {
        throw new Error("the database said no");
      }, jsonCall());
    });
    expect(answer?.status).toBe(500);
    expect(answer?.headers.get(ACTION_OUTCOME_HEADER)).toBe(null);
    expect(await answer?.text()).not.toContain("database");
  });
});

describe("a routing call in an action, through the form door", () => {
  it("answers redirect() with a 303", async () => {
    const answer = await dispatched(async () => redirect("/notes/7"), formPost());
    expect(answer.status).toBe(303);
    expect(answer.headers.get("location")).toBe("/notes/7");
  });

  it("answers the other three with their statuses and a fixed line, reporting nothing", async () => {
    const cases: $ReadOnlyArray<[() => Promise<mixed>, number, string]> = [
      [async () => notFound(), 404, "not found\n"],
      [async () => unauthorized(), 401, "unauthorized\n"],
      [async () => forbidden(), 403, "forbidden\n"],
    ];
    for (const [action, status, text] of cases) {
      let answer: Response | null = null;
      const reported = await reportedDuring(async () => {
        answer = await dispatched(action, formPost());
      });
      expect(answer?.status).toBe(status);
      expect(await answer?.text()).toBe(text);
      expect(reported).toEqual([]);
    }
  });
});

/** A page at `/notes` whose `fetch` answers every call with `answer`. */
function pageAnswering(answer: () => Response): Array<string> {
  installDom();
  globals.window.history.replaceState(null, "", "/notes");
  const loaded: Array<string> = [];
  uft.spyOn(globals.window.location, "assign").mockImplementation((href: string) => {
    loaded.push(String(href));
  });
  uft.spyOn(globals, "fetch").mockImplementation(async () => answer());
  return loaded;
}

const outcome = (status: number, kind: string, headers?: { [string]: string }) => () =>
  new Response(status === 204 ? null : JSON.stringify({ outcome: kind }), {
    status,
    headers: { [ACTION_OUTCOME_HEADER]: kind, ...headers },
  });

describe("a server action reference, when its action made a routing call", () => {
  it("throws the error the action threw, for the route's boundary", async () => {
    const cases: $ReadOnlyArray<[number, string, Class<Error>]> = [
      [404, "not-found", NotFoundError],
      [401, "unauthorized", UnauthorizedError],
      [403, "forbidden", ForbiddenError],
    ];
    for (const [status, kind, type] of cases) {
      pageAnswering(outcome(status, kind));
      let thrown: mixed = null;
      try {
        await createServerReference(ID, "app/notes/_actions.js#save")();
      } catch (error) {
        thrown = error;
      }
      expect(thrown instanceof type).toBe(true);
      uft.restoreAllMocks();
    }
  });

  it("treats an outcome it does not know as a failure", async () => {
    pageAnswering(outcome(418, "teapot"));
    let thrown: mixed = null;
    try {
      await createServerReference(ID, "app/notes/_actions.js#save")();
    } catch (error) {
      thrown = error;
    }
    expect(thrown instanceof ServerActionError).toBe(true);
  });

  it("loads the redirect's document when no router is on screen", async () => {
    const loaded = pageAnswering(outcome(204, "redirect", { location: "/notes/7" }));
    const result = await createServerReference(ID, "app/notes/_actions.js#save")();
    expect(result).toBe(undefined);
    expect(loaded).toEqual(["http://localhost/notes/7"]);
  });

  it("navigates the router on screen to the redirect, rather than loading a document", async () => {
    const save = createServerReference(ID, "app/notes/_actions.js#save");
    component Notes() {
      return (
        <main>
          <h1>notes</h1>
          <button type="button" onClick={() => void save()}>
            save
          </button>
        </main>
      );
    }
    component Note() {
      return <h1>note seven</h1>;
    }
    const loaded = pageAnswering(outcome(204, "redirect", { location: "/notes/7" }));
    const page = (path: string, file: string, module: mixed) => ({
      path,
      params: [],
      mdx: false,
      file,
      page: () => Promise.resolve({ default: module }),
      layouts: [],
      loading: [],
    });
    const table: $FlowFixMe = {
      routes: [
        page("/notes", "app/notes/$page.js", Notes),
        page("/notes/7", "app/notes/7/$page.js", Note),
      ],
      notFound: [],
      errors: [],
    };
    installRoutes(table);
    const initial = await resolveMatch(table, "/notes");
    const App = routerView("./app");
    await act(async () => {
      render(<App url="/notes" initial={initial} />);
    });

    await act(async () => {
      await userEvent.click(globals.document.querySelector("button"));
    });

    await waitFor(() =>
      expect(globals.document.querySelector("h1")?.textContent).toBe("note seven"),
    );
    expect(globals.window.location.pathname).toBe("/notes/7");
    expect(loaded).toEqual([]);
  });
});

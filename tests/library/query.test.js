// @flow
//
// `@uniflowed/query`.
//
// A cache is not interesting because it returns the right value — a variable
// does that. It is interesting because of what it refuses to do: fetch twice
// for two askers, re-render for an answer that did not change, keep an entry
// nobody is watching, let a superseded request write, or leave an optimistic
// guess on screen after the write it was guessing about failed.
//
// So almost every test here counts something: calls, renders, attempts,
// aborts. An assertion that the value arrived proves the happy path; the
// counts are what prove the cache.
//
// The value-level parts and the cache are tested without React, because they
// decide those things alone. The binding is tested through components, because
// what matters there is what React does with the snapshot it is handed.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { describe, expect, fn, it } from "@uniflowed/test";
import { act, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import {
  CancelledError,
  Mutation,
  Presence,
  QueryClient,
  QueryClientProvider,
  backoffDelay,
  hashKey,
  matchesKey,
  structuralShare,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@uniflowed/query";

const deferred = () => {
  let settle = (value: mixed) => {};
  let fail = (error: mixed) => {};
  const promise = new Promise((resolve, reject) => {
    settle = resolve;
    fail = reject;
  });
  return { promise, settle, fail };
};

/** Real time passing, outside React. */
const wait = (millis: number) => new Promise((resolve) => setTimeout(resolve, millis));

/** Real time passing, with React allowed to react to it. */
const tick = (millis: number) => act(() => wait(millis));

const withClient = (client: QueryClient, ui: React.Node) => (
  <QueryClientProvider client={client}>{ui}</QueryClientProvider>
);

describe("keys", () => {
  it("is the same request however the parameters were written", () => {
    // Two people writing the same request in two files must not produce two
    // cache entries that can disagree on screen.
    expect(hashKey(["users", { page: 1, size: 20 }])).toBe(
      hashKey(["users", { size: 20, page: 1 }]),
    );
  });

  it("does not confuse a number with the string of it", () => {
    // A route parameter is a string and a database id is a number. Treating
    // them as one entry shows the wrong data rather than raising.
    expect(hashKey(["users", 1])).not.toBe(hashKey(["users", "1"]));
  });

  it("matches a prefix, and only the whole key when asked", () => {
    expect(matchesKey(["users", 1], ["users"])).toBe(true);
    expect(matchesKey(["users", 1], ["users"], true)).toBe(false);
    expect(matchesKey(["users", 1], ["users", 1], true)).toBe(true);
    // Not a string prefix: `["user"]` is a different key, whatever the
    // serialised forms happen to share.
    expect(matchesKey(["users", 1], ["user"])).toBe(false);
  });

  it("matches the parts of a parameter record a filter names", () => {
    expect(matchesKey(["todos", { done: true, page: 2 }], ["todos", { done: true }])).toBe(true);
    expect(matchesKey(["todos", { done: false, page: 2 }], ["todos", { done: true }])).toBe(false);
  });
});

describe("structural sharing", () => {
  it("returns the previous value when the new one is deeply equal", () => {
    const before = { user: { name: "ada" }, rows: [1, 2, 3] };
    const after = { user: { name: "ada" }, rows: [1, 2, 3] };
    // The whole point: a poll that returns identical JSON must not produce a
    // new identity, or every observer re-renders twelve times a minute.
    expect(structuralShare(before, after)).toBe(before);
  });

  it("keeps the rows that did not change", () => {
    const before = { rows: [{ id: 1 }, { id: 2 }] };
    const after = { rows: [{ id: 1 }, { id: 2, done: true }] };
    const shared = structuralShare(before, after);

    expect(shared).not.toBe(before);
    // One changed row costs one re-render, not a thousand.
    expect(shared.rows[0]).toBe(before.rows[0]);
    expect(shared.rows[1]).toEqual({ id: 2, done: true });
  });

  it("does not rebuild something it cannot rebuild faithfully", () => {
    const after = { at: new Date(0) };
    const shared = structuralShare({ at: new Date(0) }, after);
    // A class instance rebuilt as an object literal has silently lost its
    // methods, so it is replaced whole.
    expect(shared.at).toBe(after.at);
  });

  it("does not resurrect a key the new value dropped", () => {
    const shared = structuralShare({ a: 1, b: 2 }, { a: 1 });
    expect(shared).toEqual({ a: 1 });
  });
});

describe("backoff", () => {
  it("doubles, and stops doubling", () => {
    expect(backoffDelay(1)).toBe(1000);
    expect(backoffDelay(2)).toBe(2000);
    expect(backoffDelay(3)).toBe(4000);
    // A tab left open overnight must not drift into hour-long silences.
    expect(backoffDelay(20)).toBe(30_000);
  });
});

describe("the cache", () => {
  it("makes one request for two askers", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    const [first, second] = await Promise.all([
      client.fetchQuery({ queryKey: ["thing"], queryFn }),
      client.fetchQuery({ queryKey: ["thing"], queryFn }),
    ]);

    expect(queryFn.mock.calls.length).toBe(1);
    expect(first).toBe("value");
    expect(second).toBe("value");
  });

  it("treats different keys as different requests", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");
    await Promise.all([
      client.fetchQuery({ queryKey: ["a"], queryFn }),
      client.fetchQuery({ queryKey: ["b"], queryFn }),
    ]);
    expect(queryFn.mock.calls.length).toBe(2);
  });

  it("keeps the previous value beside an error", async () => {
    const client = new QueryClient();
    await client.fetchQuery({ queryKey: ["thing"], queryFn: async () => "first" });
    await client
      .fetchQuery({
        queryKey: ["thing"],
        queryFn: async () => {
          throw new Error("refresh failed");
        },
        retry: false,
      })
      .catch(() => {});

    const state = client.getQueryState(["thing"]);
    // A failed refresh must not blank a page that was showing something.
    expect(state?.data).toBe("first");
    expect(state?.error?.message).toBe("refresh failed");
  });

  it("does not ask again while the answer is fresh", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");
    await client.fetchQuery({ queryKey: ["thing"], queryFn, staleTime: 60_000 });
    await client.fetchQuery({ queryKey: ["thing"], queryFn, staleTime: 60_000 });
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("counts a refresh that changed nothing as having checked", async () => {
    const client = new QueryClient();
    const rows = [{ id: 1 }];
    await client.fetchQuery({ queryKey: ["rows"], queryFn: async () => [{ id: 1 }] });
    const first = client.getQueryData(["rows"]);
    const firstState = client.getQueryState(["rows"]);

    await wait(5);
    await client.fetchQuery({ queryKey: ["rows"], queryFn: async () => rows.slice() });
    const second = client.getQueryState(["rows"]);

    // Same answer: same identity, and `dataUpdatedAt` did not move — but the
    // entry is fresh again, which is what stops a poll refetching on sight.
    expect(client.getQueryData(["rows"])).toBe(first);
    expect(second?.dataUpdatedAt).toBe(firstState?.dataUpdatedAt);
    expect((second?.checkedAt ?? 0) > (firstState?.checkedAt ?? 0)).toBe(true);
  });

  it("tries the stated number of times and no more, and reports the last error", async () => {
    const client = new QueryClient();
    let attempts = 0;
    const queryFn = fn(async () => {
      attempts += 1;
      throw new Error(`failure ${attempts}`);
    });

    let message = "";
    await client
      .fetchQuery({ queryKey: ["flaky"], queryFn, retry: 2, retryDelay: 1 })
      .catch((error) => {
        message = error.message;
      });

    // Two retries means three attempts, and the error a caller sees is the one
    // that exhausted the policy — not the first one, which was retried past.
    expect(queryFn.mock.calls.length).toBe(3);
    expect(message).toBe("failure 3");
    expect(client.getQueryState(["flaky"])?.error?.message).toBe("failure 3");
  });

  it("does not retry a failure the policy says is not worth retrying", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => {
      throw new Error("not found");
    });

    await client
      .fetchQuery({
        queryKey: ["missing"],
        queryFn,
        retry: (_count, error) => error.message !== "not found",
        retryDelay: 1,
      })
      .catch(() => {});

    // Asking a fourth time works exactly as well as the first and costs the
    // same, which is the only reason `retry` takes a predicate.
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("reports a query function that forgot to return", async () => {
    const client = new QueryClient();
    let message = "";
    await client
      .fetchQuery({ queryKey: ["thing"], queryFn: async () => undefined, retry: false })
      .catch((error) => {
        message = error.message;
      });
    expect(message).toContain("resolved with undefined");
  });

  it("aborts a cancelled request, and puts the entry back", async () => {
    const client = new QueryClient();
    const held = deferred();
    let signal: AbortSignal | null = null;

    client.setQueryData(["thing"], "cached");
    const pending = client
      .fetchQuery({
        queryKey: ["thing"],
        queryFn: (context) => {
          signal = context.signal;
          return held.promise;
        },
      })
      .catch((error) => error);

    await client.cancelQueries({ queryKey: ["thing"] });

    expect(signal?.aborted).toBe(true);
    expect(await pending).toBeInstanceOf(CancelledError);
    // Reverted: the reader keeps what they were reading, and no spinner is
    // left running by the request that was thrown away.
    expect(client.getQueryData(["thing"])).toBe("cached");
    expect(client.getQueryState(["thing"])?.fetchStatus).toBe("idle");
  });

  it("aborts the request a refetch superseded", async () => {
    const client = new QueryClient();
    const first = deferred();
    const signals: Array<AbortSignal> = [];
    const query = client.cache.build(["thing"]);

    const superseded = query
      .fetch(
        (context) => {
          signals.push(context.signal);
          return first.promise;
        },
        { retry: false, retryDelay: 0 },
      )
      .catch((error) => error);

    const replacement = await query.fetch(
      (context) => {
        signals.push(context.signal);
        return Promise.resolve("second");
      },
      { retry: false, retryDelay: 0, cancelRefetch: true },
    );

    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
    expect(replacement).toBe("second");
    expect(await superseded).toBeInstanceOf(CancelledError);

    // And the answer that was already on its way when it was superseded may
    // not write: this is the bug that only reproduces on a slow connection.
    first.settle("first");
    await wait(5);
    expect(client.getQueryData(["thing"])).toBe("second");
  });

  it("does not let a slow first answer overwrite a fast second one", async () => {
    const client = new QueryClient();
    const slow = deferred();
    const query = client.cache.build(["thing"]);

    const first = query
      .fetch(() => slow.promise, { retry: false, retryDelay: 0 })
      .catch((error) => error);
    await query.fetch(() => Promise.resolve("second"), {
      retry: false,
      retryDelay: 0,
      cancelRefetch: true,
    });
    slow.settle("first");
    await first;
    await wait(5);

    expect(client.getQueryData(["thing"])).toBe("second");
  });

  it("collects an entry nobody ever watched", async () => {
    const client = new QueryClient({ queries: { gcTime: 20 } });
    await client.prefetchQuery({ queryKey: ["thing"], queryFn: async () => "value" });
    expect(client.getQueryData(["thing"])).toBe("value");

    await wait(60);
    // A prefetch nothing ever mounted has no observer to lose, so collection
    // has to be scheduled from the moment the entry exists.
    expect(client.getQueryData(["thing"])).toBe(undefined);
  });

  it("marks every match stale and refetches only what is watched", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");
    await client.fetchQuery({ queryKey: ["users"], queryFn });
    await client.fetchQuery({ queryKey: ["users", 1], queryFn });
    await client.fetchQuery({ queryKey: ["posts"], queryFn });
    const before = queryFn.mock.calls.length;

    await client.invalidateQueries({ queryKey: ["users"] });

    expect(client.getQueryState(["users"])?.invalidated).toBe(true);
    expect(client.getQueryState(["users", 1])?.invalidated).toBe(true);
    expect(client.getQueryState(["posts"])?.invalidated).toBe(false);
    // Nothing is watching any of them, so nothing was refetched: an
    // application holding a hundred cached users must not make a hundred
    // requests because one of them changed.
    expect(queryFn.mock.calls.length).toBe(before);
  });

  it("declines to write when an updater returns undefined", () => {
    const client = new QueryClient();
    client.setQueryData(["thing"], "first");
    client.setQueryData(["thing"], () => undefined);
    expect(client.getQueryData(["thing"])).toBe("first");
  });

  it("drops the entries it is told to remove", async () => {
    const client = new QueryClient();
    await client.fetchQuery({ queryKey: ["users", 1], queryFn: async () => "ada" });
    await client.fetchQuery({ queryKey: ["posts"], queryFn: async () => "hello" });

    client.removeQueries({ queryKey: ["users"] });

    expect(client.getQueryData(["users", 1])).toBe(undefined);
    expect(client.getQueryData(["posts"])).toBe("hello");
  });

  it("counts what is in flight", async () => {
    const client = new QueryClient();
    const held = deferred();
    const pending = client.fetchQuery({ queryKey: ["thing"], queryFn: () => held.promise });
    expect(client.isFetching()).toBe(1);
    held.settle("value");
    await pending;
    expect(client.isFetching()).toBe(0);
  });
});

describe("useQuery", () => {
  component Thing(queryFn: (context: $FlowFixMe) => Promise<string>, staleTime?: number) {
    const { data, isPending, error } = useQuery({
      queryKey: ["thing"],
      queryFn,
      staleTime,
      retry: false,
    });
    if (isPending) return <output>pending</output>;
    if (error != null) return <output>{`error: ${error.message}`}</output>;
    return <output>{data ?? "nothing"}</output>;
  }

  it("shows nothing yet, then the value", async () => {
    const client = new QueryClient();
    render(withClient(client, <Thing queryFn={async () => "loaded"} />));
    expect(screen.getByText("pending")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText("loaded")).toBeInTheDocument();
    });
  });

  it("shows a cached value at once and refreshes it behind", async () => {
    const client = new QueryClient();
    await client.fetchQuery({ queryKey: ["thing"], queryFn: async () => "cached" });
    const queryFn = fn(async () => "refreshed");

    render(withClient(client, <Thing queryFn={queryFn} />));
    // Stale-while-revalidate: navigating back does not flash a spinner over
    // data that is already there.
    expect(screen.getByText("cached")).toBeInTheDocument();
    expect(screen.queryByText("pending")).toBe(null);

    await waitFor(() => {
      expect(screen.getByText("refreshed")).toBeInTheDocument();
    });
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("makes one request for two components on the same key", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "shared");

    render(
      withClient(
        client,
        <div>
          <Thing queryFn={queryFn} />
          <Thing queryFn={queryFn} />
        </div>,
      ),
    );

    await waitFor(() => {
      expect(screen.getAllByText("shared").length).toBe(2);
    });
    // A header and a sidebar showing the current user make one request, and
    // cannot disagree about the answer.
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("does not refetch while the value is fresh", async () => {
    const client = new QueryClient();
    await client.fetchQuery({ queryKey: ["thing"], queryFn: async () => "cached" });
    const queryFn = fn(async () => "refetched");

    render(withClient(client, <Thing queryFn={queryFn} staleTime={60_000} />));

    expect(screen.getByText("cached")).toBeInTheDocument();
    expect(queryFn).not.toHaveBeenCalled();
  });

  it("reports an error", async () => {
    const client = new QueryClient();
    render(
      withClient(
        client,
        <Thing
          queryFn={async () => {
            throw new Error("nope");
          }}
        />,
      ),
    );
    await waitFor(() => {
      expect(screen.getByText("error: nope")).toBeInTheDocument();
    });
  });

  it("does not resubscribe when the caller passes a new closure each render", async () => {
    const client = new QueryClient();
    const query = client.cache.build(["thing"]);
    let subscriptions = 0;
    const original = query.addObserver.bind(query);
    // $FlowFixMe[cannot-write] counting subscriptions is the point of the test.
    query.addObserver = (watcher, gcTime) => {
      subscriptions += 1;
      return original(watcher, gcTime);
    };

    component Rerendering() {
      const [count, setCount] = useState(0);
      // A fresh closure on every render, which is how everyone writes it.
      const { data } = useQuery({
        queryKey: ["thing"],
        queryFn: async () => `value ${count}`,
        staleTime: 60_000,
      });
      return (
        <button type="button" onClick={() => setCount(count + 1)}>
          {data ?? "nothing"}
        </button>
      );
    }

    render(withClient(client, <Rerendering />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("value 0");
    });
    await userEvent.click(screen.getByRole("button"));
    await userEvent.click(screen.getByRole("button"));

    // The query function is something to call, not something to react to: a
    // new closure must not tear the subscription down and set it up again.
    expect(subscriptions).toBe(1);
  });

  it("re-renders the observers of the key that changed and no others", async () => {
    const client = new QueryClient();
    client.setQueryData(["users"], ["ada"]);
    client.setQueryData(["posts"], ["hello"]);
    let userRenders = 0;
    let postRenders = 0;

    component Users() {
      userRenders += 1;
      const { data } = useQuery({
        queryKey: ["users"],
        queryFn: async () => ["ada"],
        staleTime: Number.POSITIVE_INFINITY,
      });
      return <output>{(data ?? []).join(",")}</output>;
    }
    component Posts() {
      postRenders += 1;
      const { data } = useQuery({
        queryKey: ["posts"],
        queryFn: async () => ["hello"],
        staleTime: Number.POSITIVE_INFINITY,
      });
      return <output>{(data ?? []).join(",")}</output>;
    }

    render(
      withClient(
        client,
        <div>
          <Users />
          <Posts />
        </div>,
      ),
    );
    const usersBefore = userRenders;
    const postsBefore = postRenders;

    act(() => {
      client.setQueryData(["users"], ["ada", "grace"]);
    });

    expect(screen.getByText("ada,grace")).toBeInTheDocument();
    expect(userRenders).toBe(usersBefore + 1);
    // The other entry did not move, so its snapshot did not move, so React had
    // nothing to do.
    expect(postRenders).toBe(postsBefore);
  });

  it("does not re-render when the answer is the same answer", async () => {
    const client = new QueryClient();
    client.setQueryData(["rows"], [{ id: 1 }, { id: 2 }]);
    let renders = 0;

    component Rows() {
      renders += 1;
      const { data } = useQuery({
        queryKey: ["rows"],
        queryFn: async () => [{ id: 1 }, { id: 2 }],
        staleTime: Number.POSITIVE_INFINITY,
      });
      return <output>{(data ?? []).map((row) => row.id).join(",")}</output>;
    }

    render(withClient(client, <Rows />));
    const before = renders;

    act(() => {
      // A poll answering with a fresh object graph holding identical rows.
      client.setQueryData(["rows"], [{ id: 1 }, { id: 2 }]);
    });

    expect(renders).toBe(before);
  });

  it("does not re-render when the narrowed value is unchanged", async () => {
    const client = new QueryClient();
    client.setQueryData(["user"], { name: "Ada", visits: 1 });
    let renders = 0;

    component Name() {
      renders += 1;
      const { data } = useQuery({
        queryKey: ["user"],
        queryFn: async () => ({ name: "Ada", visits: 1 }),
        staleTime: Number.POSITIVE_INFINITY,
        // Written inline, so its identity changes every render — which must
        // not be enough to defeat the memo.
        select: (user) => user.name,
      });
      return <output>{data ?? "nobody"}</output>;
    }

    render(withClient(client, <Name />));
    const before = renders;

    act(() => {
      client.setQueryData(["user"], { name: "Ada", visits: 2 });
    });
    expect(screen.getByText("Ada")).toBeInTheDocument();
    // The entry changed and the narrowed value did not, so nothing this
    // component can see changed.
    expect(renders).toBe(before);

    act(() => {
      client.setQueryData(["user"], { name: "Grace", visits: 2 });
    });
    await waitFor(() => {
      expect(screen.getByText("Grace")).toBeInTheDocument();
    });
    expect(renders).toBe(before + 1);
  });

  it("aborts the request for a key it has moved away from", async () => {
    const client = new QueryClient();
    const held = deferred();
    const signals: Array<AbortSignal> = [];
    const queryFn = fn((context) => {
      signals.push(context.signal);
      return context.queryKey[1] === "a" ? held.promise : Promise.resolve("b");
    });

    component Switcher() {
      const [id, setId] = useState("a");
      const { data } = useQuery({ queryKey: ["item", id], queryFn, retry: false });
      return (
        <button type="button" onClick={() => setId("b")}>
          {data ?? "nothing"}
        </button>
      );
    }

    render(withClient(client, <Switcher />));
    await waitFor(() => {
      expect(signals.length).toBe(1);
    });

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("b");
    });

    // Nobody is waiting for the first answer any more, and the query function
    // took the signal — so it is stopped rather than left to finish.
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
  });

  it("collects an entry after the last component leaves, unless it comes back", async () => {
    const client = new QueryClient({ queries: { gcTime: 30 } });
    const queryFn = fn(async () => "value");

    const first = render(withClient(client, <Thing queryFn={queryFn} staleTime={60_000} />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });

    // Back within the grace period: the entry is still there, the timer was
    // cancelled, and the remount costs nothing.
    first.unmount();
    await tick(10);
    const second = render(withClient(client, <Thing queryFn={queryFn} staleTime={60_000} />));
    expect(screen.getByText("value")).toBeInTheDocument();
    expect(queryFn.mock.calls.length).toBe(1);

    // Away for longer than the grace period: collected, and coming back costs
    // a request.
    second.unmount();
    await tick(60);
    expect(client.getQueryData(["thing"])).toBe(undefined);

    render(withClient(client, <Thing queryFn={queryFn} staleTime={60_000} />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });
    expect(queryFn.mock.calls.length).toBe(2);
  });

  it("does not fetch while it is disabled, and fetches when it is not", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    component Gated(enabled: boolean) {
      const { data, isPending, isFetching } = useQuery({
        queryKey: ["gated"],
        queryFn,
        enabled,
      });
      return <output>{isPending ? `pending ${String(isFetching)}` : (data ?? "nothing")}</output>;
    }

    const view = render(withClient(client, <Gated enabled={false} />));
    // Pending, but nothing is in flight: those are different facts and a
    // spinner that cannot end is what conflating them looks like.
    expect(screen.getByText("pending false")).toBeInTheDocument();
    expect(queryFn).not.toHaveBeenCalled();

    view.rerender(withClient(client, <Gated enabled={true} />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("shows placeholder data until the real answer lands", async () => {
    const client = new QueryClient();
    const held = deferred();

    component Placeheld() {
      const { data, isPlaceholderData } = useQuery({
        queryKey: ["thing"],
        queryFn: () => held.promise,
        placeholderData: "guess",
      });
      return <output>{`${String(data)} ${String(isPlaceholderData)}`}</output>;
    }

    render(withClient(client, <Placeheld />));
    expect(screen.getByText("guess true")).toBeInTheDocument();
    // Never written to the cache: it is a thing to show, not a thing known.
    expect(client.getQueryData(["thing"])).toBe(undefined);

    held.settle("real");
    await waitFor(() => {
      expect(screen.getByText("real false")).toBeInTheDocument();
    });
  });

  it("refetches when asked, and supersedes what was in flight", async () => {
    const client = new QueryClient();
    let answer = "first";
    component Refetchable() {
      const { data, refetch } = useQuery({
        queryKey: ["thing"],
        queryFn: async () => answer,
        staleTime: 60_000,
      });
      return (
        <button type="button" onClick={() => void refetch()}>
          {data ?? "nothing"}
        </button>
      );
    }
    render(withClient(client, <Refetchable />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("first");
    });

    answer = "second";
    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("second");
    });
  });

  it("finds a prefetched answer already there", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "warm");

    // What a route loader does before navigating: the component's own mount
    // then finds nothing to do.
    await client.prefetchQuery({ queryKey: ["thing"], queryFn });
    render(withClient(client, <Thing queryFn={queryFn} staleTime={60_000} />));

    expect(screen.getByText("warm")).toBeInTheDocument();
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("still makes one request when React mounts the effect twice", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    render(
      withClient(
        client,
        <React.StrictMode>
          <Thing queryFn={queryFn} staleTime={60_000} />
        </React.StrictMode>,
      ),
    );

    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });
    // Strict Mode renders twice and mounts, unmounts and remounts every
    // effect. Nothing in the render path may have created the entry, and the
    // second subscription has to join the request the first one started.
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("gives a component the client it is under", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    component Refresher() {
      const inner = useQueryClient();
      const { data } = useQuery({ queryKey: ["thing"], queryFn, staleTime: 60_000 });
      return (
        <button type="button" onClick={() => void inner.invalidateQueries({ queryKey: ["thing"] })}>
          {data ?? "nothing"}
        </button>
      );
    }

    render(withClient(client, <Refresher />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("value");
    });

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(queryFn.mock.calls.length).toBe(2);
    });
  });

  it("notices on its own when the answer goes stale", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    component Ageing() {
      const { data, isStale } = useQuery({ queryKey: ["thing"], queryFn, staleTime: 30 });
      return <output>{`${data ?? "nothing"} ${isStale ? "stale" : "fresh"}`}</output>;
    }

    render(withClient(client, <Ageing />));
    await waitFor(() => {
      expect(screen.getByText("value fresh")).toBeInTheDocument();
    });

    await tick(50);
    // Nothing happened except time passing, and a component that tells the
    // reader their data may be out of date has to be told about that.
    expect(screen.getByText("value stale")).toBeInTheDocument();
    // Going stale is not a reason to fetch. Somebody asking for it is.
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("polls when it is told to, and stops when the component goes", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    component Polling() {
      const { data } = useQuery({
        queryKey: ["thing"],
        queryFn,
        refetchInterval: 20,
        staleTime: 60_000,
      });
      return <output>{data ?? "nothing"}</output>;
    }

    const view = render(withClient(client, <Polling />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });

    await tick(70);
    expect(queryFn.mock.calls.length > 1).toBe(true);

    view.unmount();
    const before = queryFn.mock.calls.length;
    await tick(70);
    // A poll that outlives the component that asked for it is a leak with a
    // network request in it.
    expect(queryFn.mock.calls.length).toBe(before);
  });

  it("moves to a fresh entry when the one it was watching is removed", async () => {
    const client = new QueryClient();
    const queryFn = fn(async () => "value");

    render(withClient(client, <Thing queryFn={queryFn} staleTime={60_000} />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });

    act(() => {
      client.removeQueries({ queryKey: ["thing"] });
    });

    // The entry it held is gone and is not coming back. Left registered on it,
    // the component would sit on a dead object while the next fetch quietly
    // filled its replacement.
    await waitFor(() => {
      expect(queryFn.mock.calls.length).toBe(2);
    });
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });
  });

  it("says which provider is missing", () => {
    component Orphan() {
      useQuery({ queryKey: ["thing"], queryFn: async () => "value" });
      return null;
    }
    let message = "";
    try {
      render(<Orphan />);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("needs a QueryClientProvider");
  });
});

describe("useMutation", () => {
  it("restores exactly the previous data when the write fails", async () => {
    const client = new QueryClient();
    const rows = [{ id: 1, name: "ada" }];
    client.setQueryData(["users"], rows);
    const previous: $FlowFixMe = client.getQueryData(["users"]);
    const order: Array<string> = [];

    component Users() {
      const { data } = useQuery({
        queryKey: ["users"],
        queryFn: async () => rows,
        staleTime: Number.POSITIVE_INFINITY,
      });
      const create = useMutation({
        mutationFn: async () => {
          throw new Error("rejected");
        },
        onMutate: async (name: string) => {
          order.push("mutate");
          // A refetch already in flight would land after this and put the
          // server's old answer back.
          await client.cancelQueries({ queryKey: ["users"] });
          const snapshot = client.getQueryData(["users"]);
          client.setQueryData(["users"], (users: $FlowFixMe) => [...users, { id: 2, name }]);
          return { snapshot };
        },
        onError: (_error, _name, context: $FlowFixMe) => {
          order.push("error");
          client.setQueryData(["users"], context.snapshot);
        },
        onSettled: () => {
          order.push("settled");
        },
      });

      return (
        <div>
          <output>{(data ?? []).map((row) => row.name).join(",")}</output>
          <output>{create.isError ? "failed" : "fine"}</output>
          <button type="button" onClick={() => create.mutate("grace")}>
            add
          </button>
        </div>
      );
    }

    render(withClient(client, <Users />));
    expect(screen.getByText("ada")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByText("failed")).toBeInTheDocument();
    });

    const restored: $FlowFixMe = client.getQueryData(["users"]);
    expect(restored).toEqual(previous);
    // Not merely equal: the row objects are the ones that were there before
    // the guess, so a memoised row component does not re-render either.
    expect(restored[0]).toBe(previous[0]);
    expect(screen.getByText("ada")).toBeInTheDocument();
    // The rollback ran before the component was told it failed, so the reader
    // never sees the optimistic value and the failure at the same time.
    expect(order).toEqual(["mutate", "error", "settled"]);
  });

  it("shows the optimistic value while the write is in flight", async () => {
    const client = new QueryClient();
    const held = deferred();
    client.setQueryData(["users"], ["ada"]);

    component Users() {
      const { data } = useQuery({
        queryKey: ["users"],
        queryFn: async () => ["ada"],
        staleTime: Number.POSITIVE_INFINITY,
      });
      const create = useMutation({
        mutationFn: () => held.promise,
        onMutate: (name: string) => {
          client.setQueryData(["users"], (users: $FlowFixMe) => [...users, name]);
        },
      });
      return (
        <div>
          <output>{(data ?? []).join(",")}</output>
          <button type="button" onClick={() => create.mutate("grace")}>
            add
          </button>
        </div>
      );
    }

    render(withClient(client, <Users />));
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("ada,grace")).toBeInTheDocument();
    held.settle("ok");
  });

  it("invalidates what it affected once it has settled", async () => {
    const client = new QueryClient();
    let listed = ["ada"];
    const queryFn = fn(async () => listed);

    component Users() {
      const { data } = useQuery({ queryKey: ["users"], queryFn, staleTime: 60_000 });
      const create = useMutation({
        mutationFn: async (name: string) => {
          listed = [...listed, name];
          return name;
        },
        onSettled: () => client.invalidateQueries({ queryKey: ["users"] }),
      });
      return (
        <div>
          <output>{(data ?? []).join(",")}</output>
          <button type="button" onClick={() => create.mutate("grace")}>
            add
          </button>
        </div>
      );
    }

    render(withClient(client, <Users />));
    await waitFor(() => {
      expect(screen.getByText("ada")).toBeInTheDocument();
    });

    await userEvent.click(screen.getByRole("button"));
    // The list refreshed because the write invalidated its key, not because
    // the component was told to reload — and not because it went stale, since
    // it is fresh for another minute.
    await waitFor(() => {
      expect(screen.getByText("ada,grace")).toBeInTheDocument();
    });
    expect(queryFn.mock.calls.length).toBe(2);
  });

  it("rejects from mutateAsync so a caller can branch, and reports the state", async () => {
    const client = new QueryClient();
    let caught = "";

    component Failing() {
      const mutation = useMutation({
        mutationFn: async () => {
          throw new Error("rejected");
        },
      });
      return (
        <div>
          <button
            type="button"
            onClick={() => {
              mutation.mutateAsync().catch((error) => {
                caught = error.message;
              });
            }}
          >
            run
          </button>
          <output>{mutation.error?.message ?? "none"}</output>
        </div>
      );
    }

    render(withClient(client, <Failing />));
    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByText("rejected")).toBeInTheDocument();
    });
    expect(caught).toBe("rejected");
  });

  it("does not retry a write unless it is told to", async () => {
    const client = new QueryClient();
    const mutationFn = fn(async () => {
      throw new Error("nope");
    });

    component Once() {
      const mutation = useMutation({ mutationFn });
      return (
        <button type="button" onClick={() => mutation.mutate()}>
          {mutation.isError ? "failed" : "run"}
        </button>
      );
    }

    render(withClient(client, <Once />));
    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("failed");
    });
    // A repeated write creates the second invoice. Retrying one is a decision
    // about idempotency that only the caller can make.
    expect(mutationFn.mock.calls.length).toBe(1);
  });
});

describe("a mutation whose error callback throws", () => {
  it("still leaves the mutation in the error state", async () => {
    // A listener that throws is the caller's bug. A mutation left `pending`
    // for ever because of it would be this package's, and a component showing
    // a spinner has no way back from that.
    const mutation = new Mutation();

    let raised = "";
    try {
      await mutation.execute(undefined, {
        mutationFn: async () => {
          throw new Error("rejected");
        },
        onError: () => {
          throw new Error("the callback is broken too");
        },
        retry: 0,
        retryDelay: () => 0,
      });
    } catch (error) {
      raised = String(error);
    }

    expect(raised).toContain("the callback is broken too");
    expect(mutation.state.status).toBe("error");
    expect(String(mutation.state.error)).toContain("rejected");
  });
});

describe("useInfiniteQuery", () => {
  const feed = [["a", "b"], ["c", "d"], ["e"]];

  it("keeps every page in one entry and stops when the server says so", async () => {
    const client = new QueryClient();
    const queryFn = fn(async ({ pageParam }: $FlowFixMe) => ({
      items: feed[pageParam],
      next: pageParam + 1 < feed.length ? pageParam + 1 : null,
    }));

    component Feed() {
      const { data, fetchNextPage, hasNextPage } = useInfiniteQuery({
        queryKey: ["feed"],
        queryFn,
        initialPageParam: 0,
        getNextPageParam: (last: $FlowFixMe) => last.next,
        staleTime: 60_000,
      });
      const items = (data?.pages ?? []).flatMap((page) => page.items);
      return (
        <button type="button" onClick={() => void fetchNextPage()}>
          {`${items.join("")}${hasNextPage ? "+" : "."}`}
        </button>
      );
    }

    render(withClient(client, <Feed />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("ab+");
    });

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("abcd+");
    });

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      // `getNextPageParam` returned null, so the list ends without an extra
      // request to discover that.
      expect(screen.getByRole("button").textContent).toBe("abcde.");
    });
    expect(queryFn.mock.calls.length).toBe(3);

    // One entry, not three: one key, one staleness clock, one thing to
    // invalidate.
    expect(client.cache.findAll().length).toBe(1);
  });

  it("loads a page even while a refetch is in flight", async () => {
    // A page control asks for something the entry does not have. Joining the
    // request already in flight would answer with the refetch and add no page,
    // so "load more" pressed during a background refetch did nothing at all.
    const client = new QueryClient();
    const gate = deferred();
    let calls = 0;

    component Feed() {
      const { data, fetchNextPage, refetch } = useInfiniteQuery({
        queryKey: ["feed"],
        queryFn: async ({ pageParam }: $FlowFixMe) => {
          calls += 1;
          // The refetch of page 0 is held open; every other call answers.
          if (calls === 2 && pageParam === 0) {
            await gate.promise;
          }
          return {
            items: feed[pageParam],
            next: pageParam + 1 < feed.length ? pageParam + 1 : null,
          };
        },
        initialPageParam: 0,
        getNextPageParam: (last: $FlowFixMe) => last.next,
        staleTime: 60_000,
      });
      const items = (data?.pages ?? []).flatMap((page) => page.items);
      return (
        <div>
          <p data-testid="items">{items.join("")}</p>
          <button type="button" onClick={() => void refetch()}>
            refetch
          </button>
          <button type="button" onClick={() => void fetchNextPage()}>
            more
          </button>
        </div>
      );
    }

    render(withClient(client, <Feed />));
    await waitFor(() => {
      expect(screen.getByTestId("items").textContent).toBe("ab");
    });

    await userEvent.click(screen.getByRole("button", { name: "refetch" }));
    await userEvent.click(screen.getByRole("button", { name: "more" }));
    gate.settle(null);

    await waitFor(() => {
      expect(screen.getByTestId("items").textContent).toBe("abcd");
    });
  });

  it("keeps the identity of the pages it already had", async () => {
    const client = new QueryClient();
    component Feed() {
      const { data, fetchNextPage } = useInfiniteQuery({
        queryKey: ["feed"],
        queryFn: async ({ pageParam }: $FlowFixMe) => ({
          items: feed[pageParam],
          next: pageParam + 1 < feed.length ? pageParam + 1 : null,
        }),
        initialPageParam: 0,
        getNextPageParam: (last: $FlowFixMe) => last.next,
        staleTime: 60_000,
      });
      return (
        <button type="button" onClick={() => void fetchNextPage()}>
          {String((data?.pages ?? []).length)}
        </button>
      );
    }

    render(withClient(client, <Feed />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("1");
    });
    const firstPage: $FlowFixMe = (client.getQueryData(["feed"]): $FlowFixMe).pages[0];

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("2");
    });

    // Appending a page must not give every row already on screen a new
    // identity, or the whole list re-renders for one more page.
    expect((client.getQueryData(["feed"]): $FlowFixMe).pages[0]).toBe(firstPage);
  });
});

describe("presence", () => {
  it("refetches a stale entry when the reader comes back", async () => {
    const presence = new Presence();
    const client = new QueryClient({ presence });
    const queryFn = fn(async () => "value");

    component Watched() {
      const { data } = useQuery({ queryKey: ["thing"], queryFn, staleTime: 0 });
      return <output>{data ?? "nothing"}</output>;
    }

    render(withClient(client, <Watched />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });
    expect(queryFn.mock.calls.length).toBe(1);

    await act(async () => {
      presence.setFocused(false);
      presence.setFocused(true);
    });
    await waitFor(() => {
      expect(queryFn.mock.calls.length).toBe(2);
    });
  });

  it("leaves a fresh entry alone", async () => {
    const presence = new Presence();
    const client = new QueryClient({ presence });
    const queryFn = fn(async () => "value");

    component Watched() {
      const { data } = useQuery({ queryKey: ["thing"], queryFn, staleTime: 60_000 });
      return <output>{data ?? "nothing"}</output>;
    }

    render(withClient(client, <Watched />));
    await waitFor(() => {
      expect(screen.getByText("value")).toBeInTheDocument();
    });

    await act(async () => {
      presence.setFocused(false);
      presence.setFocused(true);
    });
    await tick(10);
    // Coming back to a tab that was hidden for two seconds must not refetch
    // everything: `staleTime` is already the application's statement about how
    // long an answer is good for.
    expect(queryFn.mock.calls.length).toBe(1);
  });

  it("announces a change and nothing else", () => {
    const presence = new Presence();
    const seen: Array<string> = [];
    const stop = presence.subscribe((event) => seen.push(event));

    presence.setFocused(true);
    presence.setFocused(false);
    presence.setFocused(true);
    presence.setOnline(false);
    presence.setOnline(true);
    stop();
    presence.setFocused(false);
    presence.setFocused(true);

    // Already focused, so the first call says nothing; and after the last
    // watcher leaves there is nobody to tell.
    expect(seen).toEqual(["focus", "online"]);
  });
});

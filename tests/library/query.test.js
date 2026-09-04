// @flow
//
// `@uniflowed/query`.
//
// The cache is tested on its own, because it is a plain object graph with no
// React in it — de-duplication, staleness and prefix invalidation are
// decisions it makes alone. The binding is tested through components, because
// what matters there is that two of them reading one key agree.

import * as React from "@uniflowed/react";
import { describe, expect, fn, it } from "@uniflowed/test";
import { render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import { QueryCache, QueryProvider, hash, useMutation, useQuery } from "@uniflowed/query";

const deferred = () => {
  let settle = (value: mixed) => {};
  let fail = (error: mixed) => {};
  const promise = new Promise((resolve, reject) => {
    settle = resolve;
    fail = reject;
  });
  return { promise, settle, fail };
};

describe("the cache", () => {
  it("makes one request for two askers", async () => {
    const cache = new QueryCache();
    const query = fn(async () => "value");

    const [first, second] = await Promise.all([
      cache.fetch(["thing"], query),
      cache.fetch(["thing"], query),
    ]);

    // Without this a header and a sidebar both showing the current user fetch
    // it twice, and they can disagree about the answer.
    expect(query.mock.calls.length).toBe(1);
    expect(first).toBe("value");
    expect(second).toBe("value");
  });

  it("treats different keys as different requests", async () => {
    const cache = new QueryCache();
    const query = fn(async () => "value");
    await Promise.all([cache.fetch(["a"], query), cache.fetch(["b"], query)]);
    expect(query.mock.calls.length).toBe(2);
  });

  it("does not confuse a number with the string of it", () => {
    // They are different requests, and treating them as one shows the wrong
    // data rather than raising.
    expect(hash(["users", 1])).not.toBe(hash(["users", "1"]));
  });

  it("keeps the previous value beside an error", async () => {
    const cache = new QueryCache();
    await cache.fetch(["thing"], async () => "first");
    await cache
      .fetch(["thing"], async () => {
        throw new Error("refresh failed");
      })
      .catch(() => {});

    const entry = cache.read(["thing"]);
    // A failed refresh must not blank a page that was showing something.
    expect(entry.value).toBe("first");
    expect(entry.error?.message).toBe("refresh failed");
  });

  it("is stale before anything has been fetched", () => {
    const cache = new QueryCache();
    expect(cache.isStale(["never"], 1000)).toBe(true);
  });

  it("is fresh for as long as it was told", async () => {
    const cache = new QueryCache();
    await cache.fetch(["thing"], async () => "value");
    expect(cache.isStale(["thing"], 1000)).toBe(false);
    expect(cache.isStale(["thing"], 0)).toBe(true);
  });

  it("invalidates by prefix", async () => {
    const cache = new QueryCache();
    await cache.fetch(["users"], async () => []);
    await cache.fetch(["users", 1], async () => ({ id: 1 }));
    await cache.fetch(["posts"], async () => []);

    cache.invalidate(["users"]);

    // Creating a user should refresh the list and every user under it, without
    // the caller listing them.
    expect(cache.isStale(["users"], 1000)).toBe(true);
    expect(cache.isStale(["users", 1], 1000)).toBe(true);
    expect(cache.isStale(["posts"], 1000)).toBe(false);
  });

  it("tells every watcher when a value arrives", async () => {
    const cache = new QueryCache();
    const first = fn();
    const second = fn();
    cache.subscribe(["thing"], first);
    cache.subscribe(["thing"], second);

    await cache.fetch(["thing"], async () => "value");

    expect(first.mock.calls.length > 0).toBe(true);
    expect(second.mock.calls.length > 0).toBe(true);
  });

  it("stops telling a watcher that unsubscribed", async () => {
    const cache = new QueryCache();
    const listener = fn();
    const stop = cache.subscribe(["thing"], listener);
    stop();
    await cache.fetch(["thing"], async () => "value");
    expect(listener).not.toHaveBeenCalled();
  });

  it("takes a value without running a query", () => {
    const cache = new QueryCache();
    cache.set(["thing"], "optimistic");
    expect(cache.read(["thing"]).value).toBe("optimistic");
    expect(cache.isStale(["thing"], 1000)).toBe(false);
  });
});

describe("useQuery", () => {
  const withCache = (cache, ui) => <QueryProvider cache={cache}>{ui}</QueryProvider>;

  component Thing(cache: QueryCache, query: () => Promise<string>) {
    const { value, loading, error } = useQuery(["thing"], query);
    if (loading) return <output>loading</output>;
    if (error != null) return <output>{`error: ${error.message}`}</output>;
    return <output>{value ?? "nothing"}</output>;
  }

  it("shows nothing yet, then the value", async () => {
    const cache = new QueryCache();
    render(withCache(cache, <Thing cache={cache} query={async () => "loaded"} />));
    expect(screen.getByText("loading")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText("loaded")).toBeInTheDocument();
    });
  });

  it("shows a cached value immediately, with no loading pass", async () => {
    const cache = new QueryCache();
    await cache.fetch(["thing"], async () => "cached");

    render(withCache(cache, <Thing cache={cache} query={async () => "refreshed"} />));
    // The point of stale-while-revalidate: navigating back does not flash a
    // spinner over data that is already there.
    expect(screen.getByText("cached")).toBeInTheDocument();
    expect(screen.queryByText("loading")).toBe(null);
  });

  it("reports an error", async () => {
    const cache = new QueryCache();
    render(
      withCache(
        cache,
        <Thing
          cache={cache}
          query={async () => {
            throw new Error("nope");
          }}
        />,
      ),
    );
    await waitFor(() => {
      expect(screen.getByText("error: nope")).toBeInTheDocument();
    });
  });

  it("makes one request for two components on the same key", async () => {
    const cache = new QueryCache();
    const query = fn(async () => "shared");

    render(
      withCache(
        cache,
        <div>
          <Thing cache={cache} query={query} />
          <Thing cache={cache} query={query} />
        </div>,
      ),
    );

    await waitFor(() => {
      expect(screen.getAllByText("shared").length).toBe(2);
    });
    expect(query.mock.calls.length).toBe(1);
  });

  it("does not refetch while the value is fresh", async () => {
    const cache = new QueryCache();
    await cache.fetch(["thing"], async () => "cached");
    const query = fn(async () => "refetched");

    component Fresh() {
      const { value } = useQuery(["thing"], query, { staleTime: 60_000 });
      return <output>{value ?? "nothing"}</output>;
    }
    render(withCache(cache, <Fresh />));

    expect(screen.getByText("cached")).toBeInTheDocument();
    expect(query).not.toHaveBeenCalled();
  });

  it("does not resubscribe when the caller passes a new closure each render", async () => {
    const cache = new QueryCache();
    let subscriptions = 0;
    const original = cache.subscribe.bind(cache);
    // $FlowFixMe[cannot-write] counting subscriptions is the point of the test.
    cache.subscribe = (key, listener) => {
      subscriptions += 1;
      return original(key, listener);
    };

    component Rerendering() {
      const [tick, setTick] = React.useState(0);
      // A fresh closure on every render, which is how everyone writes it.
      const { value } = useQuery(["thing"], async () => `value ${tick}`);
      return (
        <button type="button" onClick={() => setTick(tick + 1)}>
          {value ?? "nothing"}
        </button>
      );
    }

    render(withCache(cache, <Rerendering />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("value 0");
    });
    const before = subscriptions;
    await userEvent.click(screen.getByRole("button"));
    await userEvent.click(screen.getByRole("button"));

    // The query is something to call, not something to react to: a new
    // closure must not tear the subscription down and set it up again.
    expect(subscriptions).toBe(before);
  });

  it("refetches when asked", async () => {
    const cache = new QueryCache();
    let answer = "first";
    component Refetchable() {
      const { value, refetch } = useQuery(["thing"], async () => answer, {
        staleTime: 60_000,
      });
      return (
        <button type="button" onClick={() => void refetch()}>
          {value ?? "nothing"}
        </button>
      );
    }
    render(withCache(cache, <Refetchable />));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("first");
    });

    answer = "second";
    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByRole("button").textContent).toBe("second");
    });
  });

  it("says which provider is missing", () => {
    component Orphan() {
      useQuery(["thing"], async () => "value");
      return null;
    }
    let message = "";
    try {
      render(<Orphan />);
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("needs a QueryProvider");
  });
});

describe("useMutation", () => {
  it("runs, reports, and invalidates what it affected", async () => {
    const cache = new QueryCache();
    let listed = ["ada"];
    await cache.fetch(["users"], async () => listed);

    component Users() {
      const { value } = useQuery(["users"], async () => listed, { staleTime: 60_000 });
      const create = useMutation(
        async (name: string) => {
          listed = [...listed, name];
          return name;
        },
        { invalidates: [["users"]] },
      );
      return (
        <div>
          <output>{(value ?? []).join(",")}</output>
          <button type="button" onClick={() => void create.run("grace")}>
            add
          </button>
        </div>
      );
    }

    render(
      <QueryProvider cache={cache}>
        <Users />
      </QueryProvider>,
    );
    expect(screen.getByText("ada")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button"));
    // The list refreshed because the mutation invalidated its key, not because
    // the component was told to reload.
    await waitFor(() => {
      expect(screen.getByText("ada,grace")).toBeInTheDocument();
    });
  });

  it("rethrows, so a caller awaiting it can tell that it failed", async () => {
    const cache = new QueryCache();
    let caught = "";

    component Failing() {
      const mutation = useMutation(async () => {
        throw new Error("rejected");
      });
      return (
        <div>
          <button
            type="button"
            onClick={() => {
              mutation.run(undefined).catch((error) => {
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

    render(
      <QueryProvider cache={cache}>
        <Failing />
      </QueryProvider>,
    );
    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByText("rejected")).toBeInTheDocument();
    });
    expect(caught).toBe("rejected");
  });
});

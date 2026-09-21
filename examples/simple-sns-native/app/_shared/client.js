// @flow
// React's side of the service: reads as promises a component can `use`, and the
// one signal — a revision — that says they are out of date.
//
// A read belongs to a revision. A write that changed something bumps it inside a
// transition, so every screen asks again while the answers it already has stay
// on screen, and the new ones replace them together when they have arrived.

import * as React from "react";
import { createContext, startTransition, useContext, useState } from "react";

import type { Service } from "./service.js";

/** A settled read carries data or a retryable failure, never a thrown error. */

export type Settled<out T> =
  | {| readonly kind: "ready", readonly value: T |}
  | {| readonly kind: "failed" |};

async function settle<T>(read: () => Promise<T>): Promise<Settled<T>> {
  try {
    return { kind: "ready", value: await read() };
  } catch {
    return { kind: "failed" };
  }
}

type Reads = {|
  readonly read: <T>(
    revision: number,
    key: string,
    load: (Service) => Promise<T>,
  ) => Promise<Settled<T>>,
|};

/**
 * One promise per revision and key, kept outside React. A component that suspends before it has
 * mounted keeps no state or memo, so a promise it made itself would be made again on every
 * attempt and never be the one that resolved.
 */

function createReads(service: Service): Reads {
  const cache = new Map<string, Promise<Settled<mixed>>>();

  return {
    read<T>(revision: number, key: string, load: (Service) => Promise<T>): Promise<Settled<T>> {
      const name = `${revision}:${key}`;
      const known = cache.get(name);
      // The key names the read, so what is stored under it is that read's result.
      if (known != null) return known as $FlowFixMe;

      // A transition still renders the revision before this one; older ones are finished with.
      for (const stored of cache.keys()) {
        if (Number(stored.slice(0, stored.indexOf(":"))) < revision - 1) cache.delete(stored);
      }
      const started = settle(() => load(service));
      cache.set(name, started);

      return started;
    },
  };
}

type Social = {|
  readonly service: Service,
  readonly reads: Reads,
  readonly revision: number,
  readonly refresh: () => void,
|};

const Context: React.Context<Social | null> = createContext(null);

/** Mounted once, above every boundary, so it never suspends and its reads outlive a retry. */

export component SocialProvider(service: Service, children: React.Node) {
  const [reads] = useState(() => createReads(service));
  const [revision, setRevision] = useState(0);
  const refresh = () => startTransition(() => setRevision((current) => current + 1));

  return (
    <Context.Provider value={{ service, reads, revision, refresh }}>{children}</Context.Provider>
  );
}

hook useContextValue(): Social {
  const social = useContext(Context);
  if (social == null) throw new Error("SocialProvider is missing");

  return social;
}

/** The service for writes, and `refresh` for after one that changed what screens show. */

export hook useSocial(): {| readonly service: Service, readonly refresh: () => void |} {
  const { service, refresh } = useContextValue();

  return { service, refresh };
}

/**
 * A read for this revision, to hand to `AsyncRegion`. `retry` asks again after a failure; like
 * any refresh it is a transition, so the failure stays on screen until the answer is in.
 */

export hook useResource<T>(
  key: string,
  load: (Service) => Promise<T>,
): {| readonly resource: Promise<Settled<T>>, readonly retry: () => void |} {
  const { reads, revision, refresh } = useContextValue();

  return { resource: reads.read(revision, key, load), retry: refresh };
}

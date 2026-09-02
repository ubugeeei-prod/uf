// @flow
//
// Runtime-agnostic cells, implemented as ordinary Flow-typed JavaScript.

export type CellScope = "client" | "server" | "react-render" | "async-resource";
export type Unsubscribe = () => void;
type Listener = () => void;

export type CellSnapshot<+T> = {
  +scope: CellScope,
  +value: T,
};

type CellCarrier<T> = {
  +__kind: "Cell",
  +scope: CellScope,
  +get: () => T,
  +set: (T) => void,
  +subscribe: (Listener) => Unsubscribe,
};

export opaque type Cell<T> = CellCarrier<T>;

function makeCell<T>(carrier: CellCarrier<T>): Cell<T> {
  return carrier;
}

function readCarrier<T>(source: Cell<T>): CellCarrier<T> {
  return (source: any);
}

function notify(listeners: Set<Listener>): void {
  for (const listener of Array.from(listeners)) {
    listener();
  }
}

function readonlyWrite(scope: CellScope): empty {
  throw Error(`@uniflowed/cell ${scope} cells are read-only`);
}

function resourceFailed(status: "idle" | "pending" | "success" | "failure"): boolean {
  return match (status) {
    "failure" => true,
    _ => false,
  };
}

export function cell<T>(value: T): Cell<T> {
  let current = value;
  const listeners: Set<Listener> = new Set();
  return makeCell({
    __kind: "Cell",
    scope: "client",
    get: () => current,
    set: (next) => {
      current = next;
      notify(listeners);
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  });
}

export function computed<T>(derive: () => T): Cell<T> {
  return makeCell({
    __kind: "Cell",
    scope: "react-render",
    get: derive,
    set: () => readonlyWrite("react-render"),
    subscribe: () => () => {},
  });
}

export function resource<T>(load: () => Promise<T>): Cell<?T> {
  let status: "idle" | "pending" | "success" | "failure" = "idle";
  let current: ?T = null;
  let thrown: mixed = null;
  const listeners: Set<Listener> = new Set();

  function start(): void {
    if (status !== "idle") {
      return;
    }
    status = "pending";
    load().then(
      (value) => {
        status = "success";
        current = value;
        notify(listeners);
      },
      (error) => {
        status = "failure";
        thrown = error;
        notify(listeners);
      },
    );
  }

  return makeCell({
    __kind: "Cell",
    scope: "async-resource",
    get: () => {
      if (resourceFailed(status)) {
        throw thrown;
      }
      start();
      return current;
    },
    set: (next) => {
      status = "success";
      current = next;
      notify(listeners);
    },
    subscribe: (listener) => {
      listeners.add(listener);
      start();
      return () => {
        listeners.delete(listener);
      };
    },
  });
}

export function read<T>(source: Cell<T>): T {
  return readCarrier(source).get();
}

export function write<T>(source: Cell<T>, value: T): void {
  readCarrier(source).set(value);
}

export function update<T>(source: Cell<T>, reduce: (T) => T): void {
  write(source, reduce(read(source)));
}

export function subscribe<T>(source: Cell<T>, listener: () => void): Unsubscribe {
  return readCarrier(source).subscribe(listener);
}

export function snapshot<T>(source: Cell<T>): CellSnapshot<T> {
  const carrier = readCarrier(source);
  return { scope: carrier.scope, value: carrier.get() };
}

// @flow
opaque type UserId = string;
opaque type Wrapped<T> = Box<T>;

export type Status = "idle" | "loading" | "ready" | "failed";

export type User = {
  id: UserId,
  name: string,
  email?: ?string,
  tags: Array<string>,
  meta: { [key: string]: mixed },
};

export type Exact = {| +readOnly: string, -writeOnly: number, ...Rest |};

type Nested = Map<string, Array<Map<string, number>>>;

type Fn = (input: ?string, ...rest: Array<number>) => Promise<void>;

declare export function load(id: UserId): Promise<User>;

interface Serializable {
  serialize(): string;
}

class Repository<T> implements Serializable {
  #cache: Map<string, T> = new Map();
  static empty: Repository<any> = new Repository();
  serialize(): string { return JSON.stringify([...this.#cache.entries()]); }
}

function pick<T, K>(source: T, key: K): T[K] {
  return source[key];
}

const typed = new Map<string, number>();
const cast = (value: mixed) => (value: any);

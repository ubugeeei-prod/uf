// @flow

export interface Serializable {
  serialize(): string;
}

export interface Keyed<K, V> extends Serializable {
  +key: K;
  get(key: K): ?V;
}

export type Anonymous = interface { size: number };

// @flow

export class Store<T> {
  size: number;
  +name: string;
  #secret: string;

  constructor(initial: T) {
    this.size = 0;
    this.name = "store";
    this.#secret = "";
  }

  static create(): Store<mixed> {
    return new Store(null);
  }

  get current(): T {
    throw new Error("empty");
  }

  run(input: string): number {
    return input.length;
  }
}

declare class Remote {
  url: string;
  fetch(path: string): Promise<string>;
}

export type Fetcher = Remote;

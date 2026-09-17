// @flow

export type Exact = {| id: string, size: number |};

export type Inexact = { id: string, ... };

export type Optional = { id?: string, ... };

export type Covariant = { +id: string, ... };

export type Contravariant = { -sink: string, ... };

export type Indexed = { [string]: number, ... };

export type Unkeyable = { [boolean]: number, ... };

export type Methods = {
  run(input: string): number,
  get size(): number,
  set size(value: number): void,
  ...
};

export type Spread = { ...Inexact, extra: boolean, ... };

export type Callable = { (input: string): number, ... };

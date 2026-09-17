// @flow

export type Primitive = number | string | boolean | symbol | bigint;

export type Maybe = ?string;

export type Top = mixed;

export type Bottom = empty;

export type Nothing = void;

export type Literals = "a" | 42 | true | null;

export type Template = `id-${string}`;

export type Both = { kind: "a", ... } & { size: number, ... };

export type Nested = ?Array<?string>;

// @flow

export type Frozen<T> = $ReadOnly<T>;

export type List<T> = $ReadOnlyArray<T>;

export type Names<T> = $Keys<T>;

export type Held<T> = $Values<T>;

export type Present<T> = $NonMaybeType<T>;

export type Sealed<T> = $Exact<T>;

export type Difference<A, B> = $Diff<A, B>;

export type Loosened<T> = $Shape<T>;

export type Constructor<T> = Class<T>;

export type Anything = Object;

export type Mapper = $ObjMap<{ id: string, ... }, <V>(V) => V>;

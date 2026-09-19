export type MakeReadonly<T> = T extends [infer Head, ...infer Tail] ? readonly [Head, ...Tail] : T;
export type TailOf<T> = T extends [unknown, ...infer Tail] ? Tail : never;
export type ExplicitTail<T> = T extends [unknown, ...infer Tail extends string[]] ? Tail : never;

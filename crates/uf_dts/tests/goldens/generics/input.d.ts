export type Box<in out T> = { value: T };
export interface Producer<out T> {
    produce(): T;
    readonly latest: T;
    current: T;
}
export interface Consumer<in T> {
    consume(value: T): void;
}
export type Keys<T> = keyof T;
export type Values<T> = T[keyof T];
export type Property<T, K extends keyof T> = T[K];
export type Unwrap<T> = T extends Promise<infer U> ? U : T;
export type Head<T> = T extends [infer H extends string, ...infer _Rest] ? H : never;
export type Last<T extends readonly unknown[]> = T extends readonly [...infer _, infer L] ? L : undefined;
export type Nested<T> = T extends string ? "string" : T extends number ? "number" : "other";
export type CheckFunction<T> = T extends (...args: any[]) => infer R ? R : never;
export type Partialize<T> = { [K in keyof T]?: T[K] };
export type Mutable<T> = { -readonly [K in keyof T]-?: T[K] };
export type Frozen<T> = { readonly [K in keyof T]: T[K] };
export type Getters<T> = { [K in keyof T as `get${K & string}`]: () => T[K] };
export type Flags<K extends string> = { [P in K]: boolean };
export type Inferred<T> = NoInfer<T>;
export type WithThis = { method(): void } & ThisType<{ extra: number }>;
export declare const settings: { mode: "a" | "b" };
export type Settings = typeof settings;
export type Mode = (typeof settings)["mode"];
export type Utility = Partial<Settings> & Required<Settings> & Readonly<Settings> & Pick<Settings, "mode"> & Omit<Settings, "mode"> & Record<string, number>;
export type Returned = ReturnType<() => string>;
export type Settled = Awaited<Promise<string>>;
export type NonNull = NonNullable<string | null>;

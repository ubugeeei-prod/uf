export type Intrinsic = intrinsic;
export type Global = typeof globalThis;
export type GlobalMember = typeof globalThis.setTimeout;
declare function make<T>(): T;
export type Instantiated = typeof make<string>;
export type DataAttributes = {
    [key: `data-${string}`]: string;
};
export declare const expression: number;
export default expression + 1;

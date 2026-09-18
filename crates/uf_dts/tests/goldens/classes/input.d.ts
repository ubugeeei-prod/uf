declare class Base<T> {
    protected constructor(value: T);
    get value(): T;
}
export interface Service {
    start(): Promise<void>;
}
export interface MethodProperty {
    readonly execute: (input: string) => Promise<string>;
    readonly bound: (this: MethodProperty, input: string) => Promise<string>;
}
export type MethodPropertyAlias = MethodProperty;
export type OmitKeyof<TObject, TKey extends keyof any> = Omit<TObject, TKey>;
export type ReplaceReturnType<T, R> = T extends (input: infer I) => any ? (input: I) => R : never;
export interface ReplacedMethodProperty {
    readonly transform: ReplaceReturnType<(input: string) => string, Promise<number>>;
}
export declare class IndexedSource {
    read(input: string): number;
}
export interface ReplacedIndexedMethod {
    readonly read: ReplaceReturnType<IndexedSource["read"], Promise<number>>;
}
export declare class IndexedScheduler implements ReplacedIndexedMethod {
    read(input: string): Promise<number>;
}
export declare abstract class Worker<T = string> extends Base<T> implements Service {
    #secret;
    private hidden;
    protected guarded: number;
    static instances: number;
    readonly id: string;
    label?: string;
    constructor(value: T);
    constructor(value: T, id: string);
    start(): Promise<void>;
    abstract run(input: T): void;
    optional?(): void;
    static create<U>(value: U): Worker<U>;
    get size(): number;
    set size(value: number);
    [key: string]: unknown;
    static [key: string]: unknown;
    [Symbol.iterator](): Iterator<T>;
    [Symbol.dispose](): void;
}
export declare class Merged {
    fromClass: string;
}
export interface Merged {
    fromInterface: number;
}
export declare class Scheduler implements MethodProperty {
    execute(input: string): Promise<string>;
    bound(this: MethodProperty, input: string): Promise<string>;
}
export declare class OmittedScheduler implements Omit<MethodProperty, "bound"> {
    execute(input: string): Promise<string>;
}
export declare class KeyofOmittedScheduler implements OmitKeyof<MethodProperty, "bound"> {
    execute(input: string): Promise<string>;
}
export declare class PickedScheduler implements Pick<MethodProperty, "execute"> {
    execute(input: string): Promise<string>;
}
export declare class AliasScheduler implements MethodPropertyAlias {
    execute(input: string): Promise<string>;
    bound(this: MethodProperty, input: string): Promise<string>;
}
export declare class TransformScheduler implements ReplacedMethodProperty {
    transform(input: string): Promise<number>;
}
declare function mixin<T>(base: T): T;
export declare class Mixed extends mixin(Base) {
}

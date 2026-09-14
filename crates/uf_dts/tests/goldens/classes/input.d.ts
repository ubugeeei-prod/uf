declare class Base<T> {
    protected constructor(value: T);
    get value(): T;
}
export interface Service {
    start(): Promise<void>;
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
declare function mixin<T>(base: T): T;
export declare class Mixed extends mixin(Base) {
}

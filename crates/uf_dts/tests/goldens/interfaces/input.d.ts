export interface Point {
    x: number;
    y: number;
    readonly label?: string;
}

export interface Dictionary<T> {
    [key: string]: T;
    readonly [index: number]: T;
}

export interface Callable {
    (input: string): number;
    new (seed: number): Callable;
    describe(verbose?: boolean): string;
    optionalMethod?(): void;
    get size(): number;
    set size(value: number);
}

export interface Named extends Point, Dictionary<string> {
    name: string;
    "quoted-key": boolean;
    42: "answer";
}

export type Shape = {
    kind: "circle" | "square";
    area(): number;
};

export type OneLine = { a: string; b?: number };

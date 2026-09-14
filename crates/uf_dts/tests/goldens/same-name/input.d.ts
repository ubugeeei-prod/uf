export type $constructor<T> = {
    new (): T;
    init(inst: T): void;
};
export interface ZodType<out Output = unknown> {
    _output: Output;
    readonly _input: unknown;
    parse(data: unknown): Output;
}
export declare const ZodType: $constructor<ZodType>;
export interface ZodString extends ZodType<string> {
    min(length: number): this;
}
export declare const ZodString: $constructor<ZodString>;
export type ZodTypeConstructor = typeof ZodType;
declare const internal: unique symbol;
export interface Hidden {
    [internal]: true;
}
export { ZodType as Schema };

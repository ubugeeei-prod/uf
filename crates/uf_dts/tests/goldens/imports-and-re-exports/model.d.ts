export declare class Model {
    id: string;
}
export declare function createModel(): Model;
export interface Options {
    strict: boolean;
}
export interface Merged {
    a: string;
}
export declare const Merged: {
    create(): Merged;
};
declare const thing: {
    kind: "thing";
};
export default thing;

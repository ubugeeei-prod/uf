export type DescriptorType = {
    row: Record<string, any>;
    refs: any;
    unique: any[];
};
export type IncludeExp<T extends DescriptorType> = {
    [K in keyof T["refs"]]?: true;
};
export type Blog = {
    id: string;
    title: string;
};
export type Category = {
    id: string;
    name: string;
};
export type Category__Descriptor = {
    row: Category;
    refs: object;
    unique: [{ key: string }];
};
export type Blog__Descriptor = {
    row: Blog;
    refs: {
        category: Category__Descriptor;
    };
    unique: [];
};
export type TakesArray = <T extends any[]>(value: T) => void;
export type TakesArrayReference = <T extends Array<string>>(value: T) => void;

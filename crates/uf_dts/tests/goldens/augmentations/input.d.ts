import type { Base } from "some-package";
declare module "some-package" {
    interface Base {
        added: string;
    }
}
declare global {
    interface Window {
        myGlobal: string;
    }
    var counter: number;
}
export interface Uses {
    base: Base;
}
export {};

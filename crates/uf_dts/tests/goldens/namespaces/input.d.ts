export declare namespace util {
    type Maybe<T> = T | null;
    function identity<T>(value: T): T;
    const version: string;
    namespace inner {
        interface Deep {
            value: number;
        }
    }
}
export declare namespace util {
    type Extra = string;
}
export declare namespace A.B.C {
    const leaf: true;
}
export declare function configure(options: configure.Options): void;
export declare namespace configure {
    interface Options {
        verbose: boolean;
    }
}
export declare class Widget {
}
export declare namespace Widget {
    const defaults: Widget;
}
import Deep = util.inner.Deep;

export declare enum Direction {
    Up,
    Down,
    Left = 10,
    Right
}
export declare enum Color {
    Red = "RED",
    Green = "GREEN"
}
export declare const enum Bits {
    None = 0,
    A = 1 << 0,
    B = 1 << 1,
    AB = A | B
}
export declare enum Signed {
    Minus = -1,
    Zero = 0
}
export declare enum Mixed {
    A = 1,
    B = "b"
}
export declare enum Quoted {
    "not-an-identifier" = 1
}
export declare enum Computed {
    A = "a".length
}

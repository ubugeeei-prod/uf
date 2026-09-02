// @flow
export function tree() {
    const nested = {
        a: {
            b: [1, 2, 3],
        },
    };
    if (nested.a) {
        return nested
            .a
            .b;
    }
    return null;
}

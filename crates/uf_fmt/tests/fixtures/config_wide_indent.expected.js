// @flow
function outer() {
    if (condition) {
        inner();
    }
    return {
        key: [1, 2, 3],
    };
}

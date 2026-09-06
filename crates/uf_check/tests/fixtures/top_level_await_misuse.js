// @flow

// The same module with the annotation wrong. Parsing top-level `await` is not
// the same as believing whatever the module says about it: the awaited value
// is a number, and calling it a string is still an error.
const retries: string = await Promise.resolve(3);

export default retries;

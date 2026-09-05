// @flow
// A spread argument is not a function, and it is not a call either: Prettier
// walks the raw argument nodes when it decides a call is `compose(f, g)` and
// has to break, and `...xs` matches neither branch. react-native's
// helloworld/cli.flow.js is the line that noticed.

async function build(options: Options) {
  await run(assemble(...P.map((prop) => `-P${prop}`), ...options.args));
}

// Both spreads: one line, however function-shaped what is inside them is.
h(...P.map((prop) => `-P${prop}`), ...options.args);

// Only the first is a spread; the rest is an ordinary argument.
i(...P.map((prop) => `-P${prop}`), other);

// No spread in front: this really is a composition, and it really does break.
j(P.map((prop) => `-P${prop}`), ...options.args);

// Two function arguments still break, spread or not.
k(compose((a) => a, ...rest), (b) => b);

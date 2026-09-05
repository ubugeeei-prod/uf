// @flow
// Casts written as comments, the way Relay writes every generated artifact:
// the file ships in an npm package and is read by tools that do not strip
// Flow, so the cast lives in a comment and only Flow sees it.

const node = {
  argumentDefinitions: v0 /*:: as any*/,
  kind: "Fragment",
  selections: [v0 /*:: as any*/, v1 /*:: as any*/],
};

register(node /*:: as any*/);

const twice = node /*:: as any*/ /*:: as ClientQuery<Variables, Data>*/;

const spread = { ...base /*:: as any*/, extra: 1 };

// Not a comment cast: real syntax stays real syntax.
const real = value as SomeType;

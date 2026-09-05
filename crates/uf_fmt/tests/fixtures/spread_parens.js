// @flow

// A spread argument keeps its parentheses whatever the brackets around it.
// An object's spread is an `ObjectProperty` rather than a `Spread`, which is
// the only reason these two used to disagree. See ubugeeei-prod/uf#159.
const objectConditional = { ...(cond ? p : q) };
const arrayConditional = [...(cond ? [1] : [])];
const objectOr = { ...(a || b) };
const arrayOr = [...(a || b)];
const objectAnd = { ...(a && b) };
const objectNullish = { ...(a ?? b) };
const objectSequence = { ...(a, b) };

// Nothing is added where nothing was needed.
const objectMember = { ...a.b };
const objectCall = { ...f() };
const objectIdentifier = { ...a };
const arrayIdentifier = [...a];

// `??` is parenthesized in all three positions of a conditional. It binds
// tighter than `?:`, so the parentheses change nothing and `x ?? y ? a : b`
// is not a line anyone should have to work out.
const test = (x ?? y) ? a : b;
const consequent = cond ? (x ?? y) : z;
const alternate = cond ? a : (x ?? y);

// And only for `??`.
const orBranch = cond ? a || b : c;
const andBranch = cond ? a && b : c;
const nullishChain = x ?? y ?? z;
const nullishCall = f(x ?? y);

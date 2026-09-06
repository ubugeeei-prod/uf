// @flow
// A JSX element reached through an arrow body, inside a JSX expression
// container. The element asks for parentheses of its own — and so for a
// break — only when the three nodes above it are exactly the arrow, the
// call and the container. Prettier counts those positions and stops at the
// first that does not match, so anything at all in between is a different
// layout: the container's child is laid out for what it is, and the element
// goes back to being an ordinary one that fits or does not.

// The two from react-devtools-shell's LargeSubtree.js. Only the second has
// the container directly over the call.
const D = <li>{showList && arr.map((num, idx) => <li key={idx}>{num}</li>)}</li>;
const H = <li>{arr.map((num) => <li>{num}</li>)}</li>;

// Every other operator in front reads the same way as the `&&` does.
const ternary = <li>{showList ? arr.map((num, idx) => <li key={idx}>{num}</li>) : null}</li>;
const or = <li>{showList || arr.map((num, idx) => <li key={idx}>{num}</li>)}</li>;
const coalesce = <li>{showList ?? arr.map((num, idx) => <li key={idx}>{num}</li>)}</li>;
const negated = <li>{!arr.map((num) => <li>{num}</li>)}</li>;

// A node between the arrow and the call, rather than above the call.
const curried = <li>{arr.map((num) => () => <li>{num}</li>)}</li>;
const conditional = <li>{arr.map((num) => (cond ? <li>{num}</li> : null))}</li>;

// A node between the call and the container: a lookup that continues the
// chain, an enclosing call, and a spread child, which is its own kind of
// node and not a container at all.
const filtered = <li>{arr.map((num) => <li>{num}</li>).filter(Boolean)}</li>;
const nested = <li>{f(g((num) => <li>{num}</li>))}</li>;
const spread = <li>{...arr.map((num) => <li>{num}</li>)}</li>;

// `new` is not a call, and the four shapes that are the match.
const constructed = <li>{new Collection((num) => <li>{num}</li>)}</li>;
const optional = <li>{arr?.map((num) => <li>{num}</li>)}</li>;
const fragment = <>{arr.map((num) => <li>{num}</li>)}</>;
const attribute = <ul rows={arr.map((num) => <li>{num}</li>)} />;

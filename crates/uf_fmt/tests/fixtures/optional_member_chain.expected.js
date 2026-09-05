// @flow
// `a?.b(c)` is an optional call whose own `optional` flag is false — the `?.`
// belongs to the member it is called on — so a chain ending in `?.name(…)` is
// still a member chain and still breaks one link per line. Only `(a?.b)()`,
// where the parentheses end the optional chain, is an ordinary call.
// relay's MutationHandlers.js is the file that noticed.

const nodeAlreadyExistsInConnection = connection
  .getLinkedRecords(EDGES)
  ?.some((edge) => edge?.getLinkedRecord(NODE)?.getDataID() === serverNodeId);

// The same chain without the `?.`, for comparison.
const two = connection
  .getLinkedRecords(EDGES)
  .some((edge) => edge?.getLinkedRecord(NODE)?.getDataID() === serverNodeIds);

// Parenthesised: the optional chain ends at the `)`, and this is a plain call.
const a = (obj?.method)(argumentOne, argumentTwo);

// Long chains of optional calls break like any other chain.
const c = obj.first(one)?.second(two)?.third(three)?.fourth(four)?.fifth(five)?.sixth(sixSixSixSix);
const f = someObject?.someMethodName(firstArgument)?.map((element) => element.transformSomehow());

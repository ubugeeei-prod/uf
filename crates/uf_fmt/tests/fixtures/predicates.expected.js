// @flow

// Flow's two spellings of a predicate function. The inferred one has no
// return type, so the colon belongs to `%checks` — `function f(x) %checks {}`
// does not parse. See ubugeeei-prod/uf#134.
function isString(value: mixed): %checks {
  return typeof value === "string";
}

function isStringAnnotated(value: mixed): boolean %checks {
  return typeof value === "string";
}

const isNumber = (value: mixed): %checks => typeof value === "number";
const isNumberAnnotated = (value: mixed): boolean %checks => typeof value === "number";

const isArray = function (value: mixed): %checks {
  return Array.isArray(value);
};

export function isDefined(value: mixed): %checks {
  return value != null;
}

export default function (value: mixed): %checks {
  return value !== undefined;
}

async function neverAPredicate(value: mixed): Promise<boolean> {
  return typeof value === "string";
}

// A declared predicate, which always has a return type in front of it.
declare function isBoolean(value: mixed): boolean %checks(typeof value === "boolean");
declare function isLongEnough(value: mixed): boolean %checks(typeof value === "string" &&
  value.length > 12);

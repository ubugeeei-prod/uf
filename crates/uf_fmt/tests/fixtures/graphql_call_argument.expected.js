// A template passed to a call of `graphql(…)` is GraphQL too, and Prettier
// hugs the parentheses around it: the doc its embed printer produces is
// labelled, and a lone labelled argument is one a call may expand. That
// rule is what pulls the template back up beside the `(` here.
const argumentOnItsOwnLine = graphql(`
  query Q {
    field
  }
`);

const argumentBesideTheParen = graphql(`
  query Q {
    field
  }
`);

// Two arguments, so the label rule does not apply and the list breaks the
// ordinary way.
const two = graphql(
  schema,
  `
    query Q {
      field
    }
  `,
);

// A tagged template is the tag's business, not the call's; the label is
// buried inside the tagged template's doc and the call never sees it.
const tagged = useFragment(
  graphql`
    fragment F on User {
      name
    }
  `,
  ref,
);

// Not `graphql`, so the template is the author's.
const other = notGraphql(`query Q { field }`);

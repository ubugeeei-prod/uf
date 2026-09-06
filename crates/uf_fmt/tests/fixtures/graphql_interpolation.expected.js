// Prettier does not treat a template with `${}` as one document with holes
// in it. Each run of text between the holes is parsed as a whole GraphQL
// document of its own, so a trailing `${Fragment}` works and a hole in the
// middle of a selection does not.
const composed = gql`
  query Q {
    user {
      ...UserFragment
    }
  }
  ${UserFragment}
`;

const several = gql`
  query Q {
    a
  }
  ${First}
  query R {
    b
  }
  ${Second}
`;

// A blank line either side of a hole survives, one at most.
const spaced = gql`
  query Q {
    a
  }

  ${Fragment}

  query R {
    b
  }
`;

// A hole with nothing but whitespace around it: the text is not a document
// and does not have to be.
const onlyHoles = gql`
  ${First}
  ${Second}
`;

// Here the text before the hole is `query { user { ...` on its own, which
// is not a document, so the template is left exactly as written.
const holeInsideASelection = gql`
  query {
    user { ...${name} }
  }
`;

const holeInAnArgument = gql`query Q { f(a: ${value}) }`;

// A hole on a line that already holds a `#` would be commented out, so
// Prettier declines the whole template rather than decide where the
// comment ends. The rule is about the last line of the text before the
// hole, so a comment on an earlier line does not trigger it.
const holeOnACommentedLine = gql`
  query Q { a } # commented out: ${Fragment}
`;

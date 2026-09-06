// The tags Prettier reads as GraphQL, and what it does with what is inside.
const byTag = graphql`query Q { field }`;
const byShortTag = gql`query Q { field }`;
const byExperimental = graphql.experimental`query Q { field }`;
const byComment = /* GraphQL */ `query Q { field }`;

// Re-indented to the code around it, whatever the author left.
function useThing() {
  return useFragment(
    graphql`
        fragment useThingFragment on User {
          username
        }
      `,
    ref,
  );
}

// Directives hang off the node they are on and break onto their own line
// when the line does not fit; a fragment definition's go on their own line
// instead.
const directives = graphql`
  fragment F on Persona {
    basicUser {
      ...RelayClient3DModuleTestFragmentClientUser_data_withALongEnoughNameToBreak @module(name: "ClientUser.react")
      short @skip(if: $x)
    }
  }
`;

const query = graphql`
  query LongOperationNameQuery($firstVariable: String!, $secondVariable: [Int!]! = [1, 2], $third: Boolean) @dir {
    aliased: node(id: $firstVariable, filter: {name: "x", tags: [A, B], nested: {deep: true}}) {
      ... on User @include(if: $third) {
        __typename
        name
      }
      ...Spread
      ... @defer {
        lazy
      }
    }
  }
`;

// Blank lines between definitions and between selections survive, one at
// most; a comma is whitespace and goes.
const spacing = graphql`
  query A {
    a, b


    c
  }



  query B {
    d
  }
`;

// The anonymous shorthand keeps no `query` keyword.
const shorthand = graphql`{ a b }`;
const anonymousWithVariables = graphql`query ($x: Int) { f(a: $x) }`;

// An empty template collapses, however much whitespace it held.
const empty = graphql`
`;

// Not a tag Prettier reads, so the text is the author's.
const notGraphql = Relay.QL`query Q { field }`;
const alsoNot = gql.experimental`query Q { field }`;
const norThis = someGql`query Q { field }`;

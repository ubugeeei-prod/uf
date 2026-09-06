// A schema written in a template, which is how `graphql-tools` takes one.
const typeDefs = gql`
  schema @dir {
    query: MyQuery
    mutation: MyMutation

    subscription: MySubscription
  }

  "A point in time."
  scalar Date @specifiedBy(url: "https://example.com/date")
  extend scalar Date @other

  """
  A person.

  Blank lines inside a block string are kept.
  """
  type User implements Node & Actor @key(fields: "id") {
    "The identifier."
    id: ID!
    name(first: Int = 10, "How to page." after: String): [String!]!

    friends: [User] @deprecated(reason: "use connections")
    connections(
      first: Int
      after: String
      orderBy: [OrderBy!]
      includeHiddenOnes: Boolean
    ): [User!]!
  }
  extend type User @patch {
    extra: Int
  }

  interface Node {
    id: ID!
  }

  union Short = A | B
  union LongUnionName =
    | AVeryLongMemberNameHere
    | AnotherVeryLongMemberName
    | AThirdLongMemberNameHere
  extend union Short = C

  enum Color @dir {
    "Warm."
    RED
    GREEN @deprecated
    BLUE
  }
  extend enum Color {
    PURPLE
  }

  input Filter {
    "By name."
    name: String = "unnamed"
    """
    By age.
    """
    age: Int! = 3 @dir
  }
  extend input Filter {
    extra: [Int!]! = [1, 2, 3]
  }

  directive @foo(a: Int = 1, b: String) repeatable on FIELD | FRAGMENT_SPREAD | INLINE_FRAGMENT
  extend directive @foo @bar

  extend schema @link(url: "https://spec") {
    query: OtherQuery
  }
`;

// A block string value keeps its shape and is re-indented with the rest.
const withBlockValues = gql`
  query Q {
    f(
      one: """
      first line
        indented further
      """
      two: """
      single
      """
      three: """
      """
    )
  }
`;

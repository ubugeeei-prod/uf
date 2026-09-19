// @flow
import { graphql } from "@uniflowed/relay";

/** The entry query composes the screen fragment; components own all fields. */
export const screenQuery = graphql`
  query SnsScreenQuery(
    $topic: String!
    $search: String!
    $page: Int!
    $thread: ID!
    $feed: Boolean!
    $messages: Boolean!
    $settings: Boolean!
  ) {
    ...SnsScreen_query
      @arguments(
        topic: $topic
        search: $search
        page: $page
        thread: $thread
        feed: $feed
        messages: $messages
        settings: $settings
      )
  }
`;

// @flow
import { graphql } from "@uniflowed/relay";

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
    viewer {
      id
      name
      handle
      avatar
      photo
      bio
    }
    feed(topic: $topic, search: $search, page: $page) @include(if: $feed) {
      posts {
        id
        ...SnsPost_post
      }
      hasNext
    }
    threads @include(if: $messages) {
      id
      name
      handle
      avatar
      photo
      lastMessage
    }
    conversation(id: $thread) @include(if: $messages) {
      thread {
        id
        name
        handle
        avatar
        photo
        lastMessage
      }
      messages {
        id
        threadId
        author
        body
        sentAt
      }
    }
    settings @include(if: $settings) {
      id
      displayName
      handle
      bio
      email
    }
  }
`;

export const createPost = graphql`
  mutation SnsCreatePostMutation($input: PostInput!) {
    createPost(input: $input) {
      id
      ...SnsPost_post
    }
  }
`;
export const appreciate = graphql`
  mutation SnsAppreciateMutation($id: ID!, $liked: Boolean!) {
    setAppreciation(id: $id, liked: $liked) {
      id
      likes
      liked
    }
  }
`;
export const sendMessage = graphql`
  mutation SnsSendMessageMutation($input: MessageInput!) {
    sendMessage(input: $input) {
      id
      threadId
      author
      body
      sentAt
    }
  }
`;
export const updateSettings = graphql`
  mutation SnsUpdateSettingsMutation($input: SettingsInput!) {
    updateSettings(input: $input) {
      id
      displayName
      handle
      bio
      email
    }
  }
`;
export const register = graphql`
  mutation SnsRegisterMutation($input: RegisterInput!) {
    register(input: $input) {
      id
      name
      handle
      avatar
      photo
      bio
    }
  }
`;
export const login = graphql`
  mutation SnsLoginMutation($handle: String!, $password: String!) {
    login(handle: $handle, password: $password) {
      id
      name
      handle
      avatar
      photo
      bio
    }
  }
`;
export const logout = graphql`
  mutation SnsLogoutMutation {
    logout
  }
`;

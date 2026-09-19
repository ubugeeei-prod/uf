/**
 * @generated SignedSource<<db1d3cadf2f33b328313c53bed0c1cc8>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsInbox_query$fragmentType } from "./SnsInbox_query.graphql";
import type { SnsSocialFrame_query$fragmentType } from "./../../_shared/__generated__/SnsSocialFrame_query.graphql";
export type SnsMessagesPageQuery$variables = {
  thread: string,
};
export type SnsMessagesPageQuery$data = {
  readonly $fragmentSpreads: SnsInbox_query$fragmentType & SnsSocialFrame_query$fragmentType,
};
export type SnsMessagesPageQuery = {
  response: SnsMessagesPageQuery$data,
  variables: SnsMessagesPageQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "thread"
  }
],
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "name",
  "storageKey": null
},
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v3 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "photo",
  "storageKey": null
},
v4 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "avatar",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsMessagesPageQuery",
    "selections": [
      {
        "args": null,
        "kind": "FragmentSpread",
        "name": "SnsSocialFrame_query"
      },
      {
        "args": [
          {
            "kind": "Variable",
            "name": "thread",
            "variableName": "thread"
          }
        ],
        "kind": "FragmentSpread",
        "name": "SnsInbox_query"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsMessagesPageQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": [
          (v1/*:: as any*/),
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "handle",
            "storageKey": null
          },
          (v2/*:: as any*/),
          (v3/*:: as any*/),
          (v4/*:: as any*/)
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "Thread",
        "kind": "LinkedField",
        "name": "threads",
        "plural": true,
        "selections": [
          (v2/*:: as any*/),
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "lastMessage",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "concreteType": "User",
            "kind": "LinkedField",
            "name": "participant",
            "plural": false,
            "selections": [
              (v1/*:: as any*/),
              (v2/*:: as any*/),
              (v3/*:: as any*/),
              (v4/*:: as any*/)
            ],
            "storageKey": null
          }
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": [
          {
            "kind": "Variable",
            "name": "id",
            "variableName": "thread"
          }
        ],
        "concreteType": "Conversation",
        "kind": "LinkedField",
        "name": "conversation",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "concreteType": "Thread",
            "kind": "LinkedField",
            "name": "thread",
            "plural": false,
            "selections": [
              (v2/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "concreteType": "User",
                "kind": "LinkedField",
                "name": "participant",
                "plural": false,
                "selections": [
                  (v1/*:: as any*/),
                  (v2/*:: as any*/)
                ],
                "storageKey": null
              }
            ],
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "concreteType": "Message",
            "kind": "LinkedField",
            "name": "messages",
            "plural": true,
            "selections": [
              (v2/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "author",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "body",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "sentAt",
                "storageKey": null
              }
            ],
            "storageKey": null
          }
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "057b72430ed5ab08d7c66d72646b21c3",
    "id": null,
    "metadata": {},
    "name": "SnsMessagesPageQuery",
    "operationKind": "query",
    "text": "query SnsMessagesPageQuery(\n  $thread: ID!\n) {\n  ...SnsSocialFrame_query\n  ...SnsInbox_query_2omOIA\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsConversation_conversation on Conversation {\n  thread {\n    id\n    participant {\n      name\n      id\n    }\n  }\n  messages {\n    id\n    ...SnsMessage_message\n  }\n}\n\nfragment SnsInbox_query_2omOIA on Query {\n  viewer {\n    id\n  }\n  threads {\n    id\n    ...SnsThread_thread\n  }\n  conversation(id: $thread) {\n    thread {\n      id\n    }\n    ...SnsConversation_conversation\n  }\n}\n\nfragment SnsMessage_message on Message {\n  author\n  body\n  sentAt\n}\n\nfragment SnsSocialFrame_query on Query {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsThread_thread on Thread {\n  id\n  lastMessage\n  participant {\n    name\n    ...SnsAvatar_user\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "40228d00ff3d9510f324def71330e73a";

export default ((node/*:: as any*/)/*:: as Query<
  SnsMessagesPageQuery$variables,
  SnsMessagesPageQuery$data,
>*/);

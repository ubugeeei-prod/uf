/**
 * @generated SignedSource<<8fafb86623e86de3382aa414037b9983>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsConversation_conversation$fragmentType } from "./SnsConversation_conversation.graphql";
export type SnsConversationQuery$variables = {
  thread: string,
};
export type SnsConversationQuery$data = {
  readonly conversation: ?{
    readonly thread: {
      readonly id: string,
    },
    readonly $fragmentSpreads: SnsConversation_conversation$fragmentType,
  },
};
export type SnsConversationQuery = {
  response: SnsConversationQuery$data,
  variables: SnsConversationQuery$variables,
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
v1 = [
  {
    "kind": "Variable",
    "name": "id",
    "variableName": "thread"
  }
],
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsConversationQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
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
              (v2/*:: as any*/)
            ],
            "storageKey": null
          },
          {
            "args": null,
            "kind": "FragmentSpread",
            "name": "SnsConversation_conversation"
          }
        ],
        "storageKey": null
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsConversationQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
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
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "name",
                    "storageKey": null
                  },
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
    "cacheID": "62700053d6d47902a9d4b9362b1c8509",
    "id": null,
    "metadata": {},
    "name": "SnsConversationQuery",
    "operationKind": "query",
    "text": "query SnsConversationQuery(\n  $thread: ID!\n) {\n  conversation(id: $thread) {\n    thread {\n      id\n    }\n    ...SnsConversation_conversation\n  }\n}\n\nfragment SnsConversation_conversation on Conversation {\n  thread {\n    id\n    participant {\n      name\n      id\n    }\n  }\n  messages {\n    id\n    ...SnsMessage_message\n  }\n}\n\nfragment SnsMessage_message on Message {\n  author\n  body\n  sentAt\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "8d39e72e7d4a15a3a7d0c083bed17915";

export default ((node/*:: as any*/)/*:: as Query<
  SnsConversationQuery$variables,
  SnsConversationQuery$data,
>*/);

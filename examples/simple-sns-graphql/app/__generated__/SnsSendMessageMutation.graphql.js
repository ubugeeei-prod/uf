/**
 * @generated SignedSource<<8ae6a6e52a6284753e3169505928cf78>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
import type { SnsMessage_message$fragmentType } from "./SnsMessage_message.graphql";
export type MessageInput = {
  body: string,
  requestId: string,
  threadId: string,
};
export type SnsSendMessageMutation$variables = {
  input: MessageInput,
};
export type SnsSendMessageMutation$data = {
  readonly sendMessage: {
    readonly id: string,
    readonly $fragmentSpreads: SnsMessage_message$fragmentType,
  },
};
export type SnsSendMessageMutation = {
  response: SnsSendMessageMutation$data,
  variables: SnsSendMessageMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "input"
  }
],
v1 = [
  {
    "kind": "Variable",
    "name": "input",
    "variableName": "input"
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
    "name": "SnsSendMessageMutation",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": "Message",
        "kind": "LinkedField",
        "name": "sendMessage",
        "plural": false,
        "selections": [
          (v2/*:: as any*/),
          {
            "args": null,
            "kind": "FragmentSpread",
            "name": "SnsMessage_message"
          }
        ],
        "storageKey": null
      }
    ],
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsSendMessageMutation",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": "Message",
        "kind": "LinkedField",
        "name": "sendMessage",
        "plural": false,
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
    ]
  },
  "params": {
    "cacheID": "82309c23309db006856bdd2f74da5b98",
    "id": null,
    "metadata": {},
    "name": "SnsSendMessageMutation",
    "operationKind": "mutation",
    "text": "mutation SnsSendMessageMutation(\n  $input: MessageInput!\n) {\n  sendMessage(input: $input) {\n    id\n    ...SnsMessage_message\n  }\n}\n\nfragment SnsMessage_message on Message {\n  author\n  body\n  sentAt\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "22375e3020935de55fd62941e530c474";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsSendMessageMutation$variables,
  SnsSendMessageMutation$data,
>*/);

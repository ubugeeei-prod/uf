/**
 * @generated SignedSource<<658d326991c9ab85a690771fabe44f5d>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsConversation_conversation$fragmentType } from "./SnsConversation_conversation.graphql";
import type { SnsThread_thread$fragmentType } from "./SnsThread_thread.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsInbox_query$fragmentType: FragmentType;
export type SnsInbox_query$data = {
  readonly conversation: ?{
    readonly thread: {
      readonly id: string,
    },
    readonly $fragmentSpreads: SnsConversation_conversation$fragmentType,
  },
  readonly threads: ReadonlyArray<{
    readonly id: string,
    readonly $fragmentSpreads: SnsThread_thread$fragmentType,
  }>,
  readonly viewer: ?{
    readonly id: string,
  },
  readonly $fragmentType: SnsInbox_query$fragmentType,
};
export type SnsInbox_query$key = {
  readonly $data?: SnsInbox_query$data,
  readonly $fragmentSpreads: SnsInbox_query$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v1 = [
  (v0/*:: as any*/)
];
return {
  "argumentDefinitions": [
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "thread"
    }
  ],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsInbox_query",
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "User",
      "kind": "LinkedField",
      "name": "viewer",
      "plural": false,
      "selections": (v1/*:: as any*/),
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
        (v0/*:: as any*/),
        {
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsThread_thread"
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
          "selections": (v1/*:: as any*/),
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
};
})();

(node/*:: as any*/).hash = "a422a2955a888877ea55fc66145e37db";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsInbox_query$fragmentType,
  SnsInbox_query$data,
>*/);

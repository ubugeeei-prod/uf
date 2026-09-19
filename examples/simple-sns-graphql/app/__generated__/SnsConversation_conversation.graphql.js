/**
 * @generated SignedSource<<9f72c79b4443436e61803f06e290ad5d>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsMessage_message$fragmentType } from "./SnsMessage_message.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsConversation_conversation$fragmentType: FragmentType;
export type SnsConversation_conversation$data = {
  readonly messages: ReadonlyArray<{
    readonly id: string,
    readonly $fragmentSpreads: SnsMessage_message$fragmentType,
  }>,
  readonly thread: {
    readonly id: string,
    readonly participant: {
      readonly name: string,
    },
  },
  readonly $fragmentType: SnsConversation_conversation$fragmentType,
};
export type SnsConversation_conversation$key = {
  readonly $data?: SnsConversation_conversation$data,
  readonly $fragmentSpreads: SnsConversation_conversation$fragmentType,
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
};
return {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsConversation_conversation",
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "Thread",
      "kind": "LinkedField",
      "name": "thread",
      "plural": false,
      "selections": [
        (v0/*:: as any*/),
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
            }
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
        (v0/*:: as any*/),
        {
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsMessage_message"
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Conversation",
  "abstractKey": null
};
})();

(node/*:: as any*/).hash = "a05e8b74fcab88140bb225c72761c3f7";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsConversation_conversation$fragmentType,
  SnsConversation_conversation$data,
>*/);

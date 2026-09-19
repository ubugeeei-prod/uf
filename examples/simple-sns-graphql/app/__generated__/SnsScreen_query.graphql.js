/**
 * @generated SignedSource<<b11d5755a1832a7a564f9dc18b58346c>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsInbox_query$fragmentType } from "./SnsInbox_query.graphql";
import type { SnsSettings_query$fragmentType } from "./SnsSettings_query.graphql";
import type { SnsSocialFrame_query$fragmentType } from "./SnsSocialFrame_query.graphql";
import type { SnsTimeline_query$fragmentType } from "./SnsTimeline_query.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsScreen_query$fragmentType: FragmentType;
export type SnsScreen_query$data = {
  readonly inbox?: ?{
    readonly $fragmentSpreads: SnsInbox_query$fragmentType,
  },
  readonly settings?: ?{
    readonly $fragmentSpreads: SnsSettings_query$fragmentType,
  },
  readonly timeline?: ?{
    readonly $fragmentSpreads: SnsTimeline_query$fragmentType,
  },
  readonly $fragmentSpreads: SnsSocialFrame_query$fragmentType,
  readonly $fragmentType: SnsScreen_query$fragmentType,
};
export type SnsScreen_query$key = {
  readonly $data?: SnsScreen_query$data,
  readonly $fragmentSpreads: SnsScreen_query$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "feed"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "messages"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "page"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "search"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "settings"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "thread"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "topic"
    }
  ],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsScreen_query",
  "selections": [
    {
      "args": null,
      "kind": "FragmentSpread",
      "name": "SnsSocialFrame_query"
    },
    {
      "condition": "feed",
      "kind": "Condition",
      "passingValue": true,
      "selections": [
        {
          "fragment": {
            "kind": "InlineFragment",
            "selections": [
              {
                "args": [
                  {
                    "kind": "Variable",
                    "name": "page",
                    "variableName": "page"
                  },
                  {
                    "kind": "Variable",
                    "name": "search",
                    "variableName": "search"
                  },
                  {
                    "kind": "Variable",
                    "name": "topic",
                    "variableName": "topic"
                  }
                ],
                "kind": "FragmentSpread",
                "name": "SnsTimeline_query"
              }
            ],
            "type": "Query",
            "abstractKey": null
          },
          "kind": "AliasedInlineFragmentSpread",
          "name": "timeline"
        }
      ]
    },
    {
      "condition": "messages",
      "kind": "Condition",
      "passingValue": true,
      "selections": [
        {
          "fragment": {
            "kind": "InlineFragment",
            "selections": [
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
          "kind": "AliasedInlineFragmentSpread",
          "name": "inbox"
        }
      ]
    },
    {
      "condition": "settings",
      "kind": "Condition",
      "passingValue": true,
      "selections": [
        {
          "fragment": {
            "kind": "InlineFragment",
            "selections": [
              {
                "args": null,
                "kind": "FragmentSpread",
                "name": "SnsSettings_query"
              }
            ],
            "type": "Query",
            "abstractKey": null
          },
          "kind": "AliasedInlineFragmentSpread",
          "name": "settings"
        }
      ]
    }
  ],
  "type": "Query",
  "abstractKey": null
};

(node/*:: as any*/).hash = "4e59e35478ec53b81dcecce55a8a8feb";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsScreen_query$fragmentType,
  SnsScreen_query$data,
>*/);

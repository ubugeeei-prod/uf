// @flow
// Metro resolves an imported image to an asset-registry entry, which is what
// `Image` takes. uf's asset declarations describe the web, where the same import
// is a URL string, so the two meet here and nowhere else in the app.

import type { ImageSourcePropType } from "react-native";

import mika from "./media/avatars/mika.jpg";
import ren from "./media/avatars/ren.jpg";
import sora from "./media/avatars/sora.jpg";
import niko from "./media/avatars/niko.jpg";
import city from "./media/clips/city.jpg";
import rail from "./media/clips/rail.jpg";
import park from "./media/clips/park.jpg";

const asset = (image: string): ImageSourcePropType => image as $FlowFixMe;

/** Licensed portraits for the fixture members; a new account keeps its initials. */

export function portrait(id: string): ImageSourcePropType | null {
  return match (id) {
    "seed-mika" => asset(mika),
    "seed-ren" => asset(ren),
    "seed-sora" => asset(sora),
    "seed-niko" => asset(niko),
    _ => null,
  };
}

export const POSTERS = {
  city: asset(city),
  rail: asset(rail),
  park: asset(park),
};

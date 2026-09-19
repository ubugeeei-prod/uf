// @flow
// The app owns the Testing Library and renderer versions. v14 uses the
// successor Test Renderer; its async API is re-exported without emulation.
export * from "@testing-library/react-native";
export type * from "@testing-library/react-native";
import { fireEvent } from "@testing-library/react-native";

export const press = fireEvent.press;
export const changeText = fireEvent.changeText;
export const scroll = fireEvent.scroll;

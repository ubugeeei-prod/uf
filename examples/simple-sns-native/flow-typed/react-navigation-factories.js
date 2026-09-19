// @flow
// The app uses only React Navigation's no-config, dynamic navigator factories.
// Keep this boundary explicit: its TypeScript state-bag constraints rely on
// mutable-array covariance, which cannot be represented faithfully in Flow.
// Screen paths and parameters are owned by the generated uf route table.
declare module "@react-navigation/native-stack" {
  declare export function createNativeStackNavigator(): interface {
    readonly Navigator: React$ComponentType<{ ... }>,
    readonly Screen: React$ComponentType<{ ... }>,
  };
}

declare module "@react-navigation/bottom-tabs" {
  declare export function createBottomTabNavigator(): interface {
    readonly Navigator: React$ComponentType<{ ... }>,
    readonly Screen: React$ComponentType<{ ... }>,
  };
}

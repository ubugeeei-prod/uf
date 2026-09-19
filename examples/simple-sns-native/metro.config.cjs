// @noflow
const { getDefaultConfig } = require("expo/metro-config");
const { withUniflowedMetro } = require("@uniflowed/react-native/metro");
module.exports = withUniflowedMetro({ ...getDefaultConfig(__dirname), maxWorkers: 2 });

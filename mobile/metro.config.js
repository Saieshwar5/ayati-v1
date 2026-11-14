// The app imports ../shared as @cuckoo/shared (a file: dependency, so npm
// links it). Metro watches that folder too.
const path = require('path');
const { getDefaultConfig } = require('expo/metro-config');

const config = getDefaultConfig(__dirname);
config.watchFolders = [path.resolve(__dirname, '../shared')];
module.exports = config;

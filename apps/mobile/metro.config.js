const path = require('path')
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config')
const workspaceRoot = path.resolve(__dirname, '../..')
module.exports = mergeConfig(getDefaultConfig(__dirname), {
  watchFolders: [workspaceRoot],
  resolver: {
    blockList: [/\/target\/.*/, /\/reference\/.*/, /\/test-results\/.*/],
    nodeModulesPaths: [path.join(__dirname, 'node_modules'), path.join(workspaceRoot, 'node_modules')],
  },
})

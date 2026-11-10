import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const tauri = path.join(root, 'apps/desktop-tauri/src-tauri')
const source = path.join(tauri, 'shell/macos')
const bundle = path.join(tauri, '.generated/AnyDropFinder.appex')
const contents = path.join(bundle, 'Contents')
mkdirSync(path.join(contents, 'MacOS'), { recursive: true })
copyFileSync(path.join(source, 'FinderSync-Info.plist'), path.join(contents, 'Info.plist'))
if (process.env.GITHUB_RUN_NUMBER) {
  execFileSync('/usr/libexec/PlistBuddy', [
    '-c',
    `Set :CFBundleVersion 1.${process.env.GITHUB_RUN_NUMBER}.${process.env.GITHUB_RUN_ATTEMPT || '1'}`,
    path.join(contents, 'Info.plist'),
  ])
}
execFileSync(
  'xcrun',
  [
    'clang',
    '-fobjc-arc',
    '-fmodules',
    '-fapplication-extension',
    '-arch',
    'arm64',
    '-arch',
    'x86_64',
    '-mmacosx-version-min=10.13',
    '-framework',
    'Cocoa',
    '-framework',
    'FinderSync',
    '-e',
    '_NSExtensionMain',
    path.join(source, 'FinderSync.m'),
    '-o',
    path.join(contents, 'MacOS/AnyDropFinder'),
  ],
  { stdio: 'inherit' }
)
execFileSync(
  '/usr/bin/codesign',
  [
    '--force',
    '--sign',
    process.env.APPLE_SIGNING_IDENTITY || '-',
    '--entitlements',
    path.join(source, 'FinderSync.entitlements'),
    bundle,
  ],
  { stdio: 'inherit' }
)
execFileSync('/usr/bin/codesign', ['--verify', '--strict', bundle], { stdio: 'inherit' })
console.log('Built signed universal Finder Sync extension')

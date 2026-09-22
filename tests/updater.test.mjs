import assert from 'node:assert/strict'
import test from 'node:test'

import { updaterConfig } from '../scripts/prepare-updater.mjs'
import { manifest } from '../scripts/updater-manifest.mjs'

test('unconfigured builds work; partial signing configuration fails closed', () => {
  assert.equal(updaterConfig({}, '0.1.2').bundle.createUpdaterArtifacts, false)
  assert.throws(() => updaterConfig({ ANYDROP_UPDATER_PUBLIC_KEY: 'test' }, '0.1.2'), /incomplete/)
  assert.throws(
    () =>
      updaterConfig(
        {
          ANYDROP_UPDATER_PUBLIC_KEY: 'test',
          ANYDROP_UPDATER_ENDPOINT: 'http://localhost/update',
          TAURI_SIGNING_PRIVATE_KEY: 'private-test',
        },
        '0.1.2'
      ),
    /HTTPS/
  )
  const config = updaterConfig(
    {
      ANYDROP_UPDATER_PUBLIC_KEY: 'public-test',
      ANYDROP_UPDATER_ENDPOINT: 'https://example.invalid/update',
      TAURI_SIGNING_PRIVATE_KEY: 'private-test',
    },
    '0.1.2-rc.42.1'
  )
  assert.equal(config.bundle.createUpdaterArtifacts, true)
  assert(!JSON.stringify(config).includes('private-test'))
})
test('universal macOS artifact maps to both native updater architectures with signature content', () => {
  const data = manifest({
    version: '0.1.2-rc.42.1',
    baseUrl: 'https://example.invalid/release',
    windows: { name: 'AnyDrop Setup.exe', signature: ' win-sig\n' },
    macos: { name: 'AnyDrop.app.tar.gz', signature: ' mac-sig\n' },
  })
  assert.deepEqual(Object.keys(data.platforms).sort(), ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64'])
  assert.deepEqual(data.platforms['darwin-aarch64'], data.platforms['darwin-x86_64'])
  assert.equal(data.platforms['windows-x86_64'].signature, 'win-sig')
  assert.equal(data.platforms['windows-x86_64'].url, 'https://example.invalid/release/AnyDrop%20Setup.exe')
  assert.throws(
    () => manifest({ version: '0.1.2', baseUrl: 'https://example.invalid', windows: { signature: '' } }),
    /signed/
  )
})

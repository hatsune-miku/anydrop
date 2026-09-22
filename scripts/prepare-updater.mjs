import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

// Outputs public build configuration only. The signing secret stays in the Tauri process environment.
export function updaterConfig(env, version) {
  const key = (env.ANYDROP_UPDATER_PUBLIC_KEY || '').trim()
  const endpoint = (env.ANYDROP_UPDATER_ENDPOINT || '').trim()
  const signing = Boolean(env.TAURI_SIGNING_PRIVATE_KEY)
  const enabled = Boolean(key && endpoint && signing)
  if ((key || endpoint || signing) && !enabled)
    throw new Error(
      'Updater configuration is incomplete: public key, HTTPS endpoint and signing secret are all required'
    )
  if (endpoint && new URL(endpoint).protocol !== 'https:') throw new Error('Updater endpoint must use HTTPS')
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error('A valid release SemVer is required')
  return {
    version,
    bundle: { createUpdaterArtifacts: enabled },
    plugins: { updater: { pubkey: key, endpoints: endpoint ? [endpoint] : [], windows: { installMode: 'passive' } } },
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === import.meta.filename) {
  const app = JSON.parse(await readFile('apps/desktop-tauri/src-tauri/tauri.conf.json', 'utf8'))
  const version = process.env.ANYDROP_RELEASE_VERSION || app.version
  const config = updaterConfig(process.env, version)
  const file = 'apps/desktop-tauri/src-tauri/.generated/updater.json'
  await mkdir(path.dirname(file), { recursive: true })
  await writeFile(file, JSON.stringify(config, null, 2) + '\n')
  if (process.env.GITHUB_OUTPUT)
    await appendFile(process.env.GITHUB_OUTPUT, `enabled=${config.bundle.createUpdaterArtifacts}\n`)
  console.log(
    `Prepared ${version}; signed updater artifacts ${config.bundle.createUpdaterArtifacts ? 'enabled' : 'disabled'}`
  )
}

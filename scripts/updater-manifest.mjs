import { readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'

async function filesAt(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = await Promise.all(
    entries.map((entry) =>
      entry.isDirectory() ? filesAt(path.join(directory, entry.name)) : [path.join(directory, entry.name)]
    )
  )
  return files.flat()
}

export function manifest({ version, windows, macos, baseUrl, notes = '' }) {
  const base = new URL(baseUrl)
  if (base.protocol !== 'https:') throw new Error('Update artifacts must use HTTPS')
  if (!windows?.signature.trim() || !macos?.signature.trim())
    throw new Error('Both signed desktop artifacts are required')
  function platform(artifact) {
    return {
      url: new URL(encodeURIComponent(artifact.name), base.href.endsWith('/') ? base : base.href + '/').href,
      signature: artifact.signature.trim(),
    }
  }
  return {
    version,
    notes,
    platforms: {
      'windows-x86_64': platform(windows),
      // The same universal archive runs on both architectures, but updater targets are separate.
      'darwin-aarch64': platform(macos),
      'darwin-x86_64': platform(macos),
    },
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === import.meta.filename) {
  const [directory, version, baseUrl, destination = 'latest.json'] = process.argv.slice(2)
  if (!directory || !version || !baseUrl)
    throw new Error('Usage: updater-manifest.mjs ARTIFACTS VERSION HTTPS_RELEASE_URL [OUTPUT]')
  const files = await filesAt(directory)
  async function artifact(suffix) {
    const matches = files.filter((file) => file.endsWith(suffix))
    if (matches.length !== 1) throw new Error(`Expected exactly one ${suffix} artifact; got ${matches.length}`)
    const file = matches[0]
    return { name: path.basename(file), signature: await readFile(file + '.sig', 'utf8') }
  }
  const result = manifest({ version, baseUrl, windows: await artifact('.exe'), macos: await artifact('.app.tar.gz') })
  await writeFile(destination, JSON.stringify({ ...result, pub_date: new Date().toISOString() }, null, 2) + '\n')
  console.log(`Created updater manifest for ${version}`)
}

// Exercise an isolated native debug app; never writes the user's clipboard.
// Build: cargo build -p anydrop-desktop-tauri --features tauri/custom-protocol
//        cargo build -p anydrop --example desktop_file_probe
// Run: node scripts/local-api-check.mjs APP_BINARY FILE_PROBE_BINARY
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import dgram from 'node:dgram'
import fs from 'node:fs/promises'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

const appBinary = path.resolve(process.argv[2] || 'target/debug/anydrop-desktop-tauri')
const fileProbe = path.resolve(process.argv[3] || 'target/debug/examples/desktop_file_probe')
const root = await fs.mkdtemp(path.join(os.tmpdir(), 'anydrop-local-api-'))
const output = path.resolve('test-results/local-api')
await fs.mkdir(output, { recursive: true })
let app
let sender
let log = ''
let senderLog = ''
const checks = []

async function freePort() {
  const tcp = net.createServer()
  await new Promise((resolve, reject) => {
    tcp.once('error', reject)
    tcp.listen(0, '127.0.0.1', resolve)
  })
  const port = tcp.address().port
  const udp = dgram.createSocket('udp4')
  try {
    await new Promise((resolve, reject) => {
      udp.once('error', reject)
      udp.bind(port, '127.0.0.1', resolve)
    })
  } finally {
    udp.close()
    await new Promise((resolve) => tcp.close(resolve))
  }
  return port
}

async function eventually(action, message) {
  let last
  for (let attempt = 0; attempt < 150; attempt += 1) {
    try {
      const result = await action()
      if (result) return result
    } catch (error) {
      last = error
    }
    if (app?.exitCode != null) throw new Error(`App exited (${app.exitCode}): ${log}`)
    await delay(100)
  }
  throw new Error(`${message}: ${last || log}`)
}

try {
  const apiPort = await freePort()
  const dataPort = await freePort()
  const discoveryPort = await freePort()
  const endpoint = `http://127.0.0.1:${apiPort}`
  const destination = path.join(root, 'received chosen')
  const settings = {
    sendClipboardEnabled: false,
    receiveClipboardEnabled: false,
    sendOnlyOnDoubleCopy: false,
    groupIdentity: 3947801231,
    discoveryPort,
    dataPort,
    displayName: `API verification ${randomUUID()}`,
    syncImageEnabled: false,
    darkMode: null,
    defaultSaveDir: path.join(root, 'received default'),
    clipboardPopupEnabled: false,
    deviceId: randomUUID(),
    suppressPopupInGame: true,
  }
  await fs.writeFile(path.join(root, 'settings.json'), JSON.stringify(settings))
  await fs.writeFile(
    path.join(root, 'desktop-settings.json'),
    JSON.stringify({ clipboardShortcut: '', autoReceiveFiles: false, fileContextMenuEnabled: false })
  )
  app = spawn(appBinary, ['--autostart'], {
    env: { ...process.env, ANYDROP_TEST_CONFIG_DIR: root, ANYDROP_TEST_API_PORT: String(apiPort) },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  app.stdout.on('data', (chunk) => (log += chunk.toString()))
  app.stderr.on('data', (chunk) => (log += chunk.toString()))
  app.on('error', (error) => (log += error.stack))

  async function call(method, params = {}, timeout = 5000) {
    const response = await fetch(`${endpoint}/api/${method}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
      signal: AbortSignal.timeout(timeout),
    })
    const value = await response.json()
    assert.equal(value.ok, true, JSON.stringify(value))
    return value.result
  }

  async function mcp(method, params) {
    const response = await fetch(`${endpoint}/mcp`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json, text/event-stream',
        'MCP-Protocol-Version': '2025-11-25',
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: randomUUID(), method, params }),
      signal: AbortSignal.timeout(15000),
    })
    const value = await response.json()
    assert.equal(value.error, undefined, JSON.stringify(value))
    return value.result
  }

  await eventually(async () => (await call('get_snapshot')).running, 'Native service did not start')
  const inventory = await (await fetch(`${endpoint}/api`)).json()
  await fs.writeFile(path.join(output, 'operations.json'), JSON.stringify(inventory, null, 2))
  const initialized = await mcp('initialize', {
    protocolVersion: '2025-11-25',
    capabilities: {},
    clientInfo: { name: 'AnyDrop native API probe', version: '1' },
  })
  assert.equal(initialized.protocolVersion, '2025-11-25')
  const tools = await mcp('tools/list', {})
  assert.deepEqual(tools.tools.map((tool) => tool.name), inventory.operations.map((tool) => tool.name))
  const nativeSnapshot = await mcp('tools/call', { name: 'get_snapshot', arguments: {} })
  assert.equal(nativeSnapshot.structuredContent.result.settings.displayName, settings.displayName)
  const ipv6 = await (await fetch(`http://[::1]:${apiPort}/api`)).json()
  assert.equal(ipv6.name, 'AnyDrop')
  checks.push('MCP handshake/catalogue/native calls, HTTP catalogue, IPv4 and IPv6')

  await call('save_settings', { settings: { darkMode: true } })
  const changed = await call('save_settings', { settings: { darkMode: null } })
  assert.equal(changed.settings.darkMode, null)
  assert.equal(changed.settings.groupIdentity, settings.groupIdentity)
  assert.equal(changed.settings.dataPort, dataPort)
  const preferences = await call('get_desktop_preferences')
  assert.equal(preferences.autoReceiveFiles, false)
  assert.equal((await call('set_auto_receive_files', { enabled: false })).autoReceiveFiles, false)
  await call('set_peer_remark', { peerKey: 'probe-peer', remark: 'Temporary nickname' })
  await call('set_peer_remark', { peerKey: 'probe-peer', remark: '' })
  assert.deepEqual(JSON.parse(await fs.readFile(path.join(root, 'peer_remarks.json'), 'utf8')), {})
  checks.push('Partial GUI settings, system theme, desktop preferences and peer remarks')

  const file = path.join(root, '中文 空格.txt')
  const folder = path.join(root, "folder '$()")
  await fs.mkdir(path.join(folder, 'nested'), { recursive: true })
  const contents = 'Native API transfer\n完整文件内容\n'.repeat(1000)
  await fs.writeFile(file, contents)
  await fs.writeFile(path.join(folder, 'nested', 'two.txt'), 'second selection')
  const { cursor } = await call('get_event_cursor')
  let completedEarly = false
  const completed = mcp('tools/call', {
    name: 'wait_for_file',
    arguments: { after: cursor, timeoutMs: 12000 },
  }).then((result) => {
    completedEarly = true
    assert.equal(result.isError, false, JSON.stringify(result))
    return result.structuredContent.result
  })
  sender = spawn(fileProbe, [String(dataPort), file, folder], { stdio: ['ignore', 'pipe', 'pipe'] })
  sender.stdout.on('data', (chunk) => (senderLog += chunk.toString()))
  sender.stderr.on('data', (chunk) => (senderLog += chunk.toString()))
  const offered = await call('wait_for_file', { stage: 'offered', after: cursor, timeoutMs: 10000 }, 12000)
  assert.equal(offered.timedOut, false)
  assert.equal(offered.event.data.entries.length, 2)
  await delay(200)
  assert.equal(completedEarly, false, 'Default file wait must not return an unaccepted offer')
  await call('accept_transfer', { transferKey: offered.event.data.transferKey, saveDir: destination })
  const received = await completed
  assert.equal(received.timedOut, false)
  assert.equal(received.event.data.transferKey, offered.event.data.transferKey)
  assert.deepEqual(
    received.event.data.paths.map((value) => path.normalize(value)).sort(),
    [path.join(destination, path.basename(file)), path.join(destination, path.basename(folder))].sort()
  )
  assert.equal(await fs.readFile(path.join(destination, path.basename(file)), 'utf8'), contents)
  assert.equal(await fs.readFile(path.join(destination, path.basename(folder), 'nested', 'two.txt'), 'utf8'), 'second selection')
  await eventually(() => sender.exitCode === 0, 'File sender did not complete')
  const replay = await call('wait_for_file', { after: cursor, timeoutMs: 100 })
  assert.deepEqual(replay, received)
  const noText = await call('wait_for_clipboard_text', { after: cursor, timeoutMs: 100 })
  assert.equal(noText.timedOut, true)
  for (const method of ['pause_transfer', 'resume_transfer', 'cancel_transfer', 'reject_transfer']) {
    const invalid = await fetch(`${endpoint}/api/${method}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ transferKey: offered.event.data.transferKey }),
    })
    assert.equal(invalid.status, 400, `${method} must reject completed transfers`)
  }
  assert.equal((await call('get_snapshot')).transfers[0].status, 7)
  checks.push('Real QUIC offer, explicit acceptance, default completion wait, multiple local paths, exact file contents, cursor replay')

  await call('dismiss_popup')
  assert.equal((await call('clear_transfers')).transfers.length, 0)
  await call('stop_service', {}, 15000)
  assert.equal((await call('get_snapshot')).running, false)
  await call('start_service')
  await eventually(async () => (await call('get_snapshot')).running, 'LAN service did not restart')
  assert.equal((await call('get_app_info')).name, 'AnyDrop')
  checks.push('History clearing, LAN stop/start while local API stays available')
  await call('quit_app')
  for (let attempt = 0; attempt < 50 && app.exitCode === null; attempt += 1) await delay(100)
  assert.equal(app.exitCode, 0, log)
  checks.push('Graceful application exit')
  await fs.writeFile(path.join(output, 'native-verification.json'), JSON.stringify({ passed: true, checks, clipboardModified: false }, null, 2))
  console.log(checks.map((check) => `PASS: ${check}`).join('\n'))
} finally {
  if (sender && sender.exitCode === null) sender.kill('SIGTERM')
  if (app && app.exitCode === null) app.kill('SIGTERM')
  await fs.writeFile(path.join(output, 'native-app.log'), log)
  await fs.writeFile(path.join(output, 'native-sender.log'), senderLog)
  await fs.rm(root, { recursive: true, force: true })
}

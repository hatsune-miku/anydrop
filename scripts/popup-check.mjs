import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'

import { epoch, installMock, snapshot } from '../tests/visual/fixture.mjs'

const engine = process.env.VISUAL_ENGINE || 'chromium'
const browser = await { chromium, webkit }[engine].launch()
await mkdir('test-results/desktop', { recursive: true })
try {
  for (const dark of [false, true]) {
    const snap = snapshot(dark)
    snap.transfers = []
    const page = await browser.newPage({ viewport: { width: 540, height: 690 }, colorScheme: dark ? 'dark' : 'light' })
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.addInitScript(installMock, { snap, label: 'receive', platform: 'MacIntel', now: epoch })
    await page.addInitScript(() => {
      window.__outgoingRequests = [{ id: 'clip-1', clipboard: true, fileName: '剪贴板.txt', bytes: 4194305 }]
    })
    await page.goto('http://127.0.0.1:1420/?window=receive')
    await page.getByText('确认以文件形式发送', { exact: true }).waitFor()
    const send = page.getByRole('button', { name: '确认发送', exact: true })
    assert(await send.isDisabled(), 'Multiple peers require explicit selection')
    await page.getByRole('group', { name: '选择接收设备' }).getByRole('button').first().click()
    assert(await send.isEnabled())
    await page.evaluate(() => {
      window.__outgoingFailure = true
    })
    await send.click()
    await page.getByRole('alert').filter({ hasText: '所选设备已离线' }).waitFor()
    await page.evaluate(() => {
      window.__outgoingFailure = false
    })
    const styles = await page.evaluate(() => ({
      background: getComputedStyle(document.documentElement).backgroundColor,
      scheme: getComputedStyle(document.documentElement).colorScheme,
      gutter: getComputedStyle(document.querySelector('.popup-body')).padding,
      card: document.querySelector('.popup-card').getBoundingClientRect().toJSON(),
    }))
    assert.equal(styles.background, 'rgba(0, 0, 0, 0)')
    assert.equal(styles.scheme, 'light')
    assert.equal(styles.gutter, '96px')
    assert.equal(styles.card.width, 348)
    await page.evaluate(() =>
      window.__emit('clipboard-popup', {
        id: 1,
        kind: 'text',
        title: '收到 JSON (12,345 字符)',
        preview: '{"hello":"世界"}',
        text: '{"hello":"世界"}',
        peer: 'phone',
      })
    )
    await page.getByText('收到 JSON (12,345 字符)', { exact: true }).waitFor()
    await page.screenshot({
      path: `test-results/desktop/popup-${engine}-${dark ? 'dark' : 'light'}.png`,
      omitBackground: true,
    })
    await page.getByRole('button', { name: '取消', exact: true }).click()
    await page.getByText('确认以文件形式发送', { exact: true }).waitFor({ state: 'hidden' })
    const before = await page.evaluate(
      () => window.__testCalls.filter((c) => c.cmd === 'confirm_outgoing_request').length
    )
    assert.equal(before, 1, 'Cancel does not submit another offer')
    await page.evaluate((data) => {
      data.peers = data.peers.slice(0, 1)
      window.__emit('snapshot', data)
      window.__outgoingRequests = [{ id: 'clip-2', clipboard: true, fileName: '新剪贴板.png', bytes: 5000000 }]
      window.__emit('outgoing-requests', window.__outgoingRequests)
    }, snap)
    await page.getByText('新剪贴板.png', { exact: false }).waitFor()
    assert(await send.isEnabled(), 'Single peer needs only confirmation')
    await send.click()
    await page.getByText('确认以文件形式发送', { exact: true }).waitFor({ state: 'hidden' })
    assert.equal(
      (await page.evaluate(() => window.__testCalls.filter((c) => c.cmd === 'confirm_outgoing_request').at(-1))).args
        .peerName,
      snap.peers[0].name
    )
    await page.evaluate((data) => {
      data.peers = []
      window.__emit('snapshot', data)
      window.__outgoingRequests = [{ id: 'clip-3', clipboard: true, fileName: '等待.txt', bytes: 5000000 }]
      window.__emit('outgoing-requests', window.__outgoingRequests)
    }, snap)
    await page.getByText('当前频段暂无在线设备').waitFor()
    assert(await send.isDisabled(), 'No peer cannot send')
    await page.evaluate(() => {
      window.__emit('clipboard-popup', { id: 8, kind: 'text', preview: 'API dismissal', text: 'API dismissal', peer: 'test' })
      window.__emit('dismiss-popup', { receiptId: null, transferKeys: [], allReceipts: true })
    })
    await page.locator('.popup-card--clip').waitFor({ state: 'hidden' })
    assert(await page.getByText('等待.txt', { exact: false }).isVisible(), 'API dismissal keeps pending confirmations')
    assert.deepEqual(errors, [])
    await page.close()
  }
  // Slow first hydration plus a newer event must neither hide nor erase the request.
  const late = await browser.newPage({ viewport: { width: 540, height: 500 } })
  const lateSnapshot = snapshot(false)
  lateSnapshot.transfers = []
  await late.addInitScript(installMock, { snap: lateSnapshot, label: 'receive', platform: 'Win32', now: epoch })
  await late.addInitScript(() => {
    window.__holdOutgoingRead = true
  })
  await late.goto('http://127.0.0.1:1420/?window=receive')
  await late.waitForFunction(
    () =>
      typeof window.__finishOutgoingRead === 'function' &&
      window.__testCalls.some((c) => c.cmd === 'plugin:window|hide')
  )
  await late.evaluate(() => {
    window.__outgoingRequests = [{ id: 'late', clipboard: true, fileName: 'late.txt', bytes: 5000000 }]
    window.__emit('outgoing-requests', window.__outgoingRequests)
    window.__finishOutgoingRead()
  })
  await late.getByText('确认以文件形式发送', { exact: true }).waitFor()
  await late.waitForFunction(() => window.__testCalls.some((c) => c.cmd === 'show_receive_popup'))
  assert(
    await late.getByText('late.txt', { exact: false }).isVisible(),
    'Stale initial state cannot overwrite a newer request'
  )
  await late.close()
  // A long-lived popup must recover a lost menu event without being reloaded.
  const missed = await browser.newPage({ viewport: { width: 540, height: 690 } })
  await missed.addInitScript(installMock, { snap: lateSnapshot, label: 'receive', platform: 'Win32', now: epoch })
  await missed.goto('http://127.0.0.1:1420/?window=receive')
  await missed.waitForFunction(() => window.__testCalls.some((c) => c.cmd === 'plugin:window|hide'))
  await missed.evaluate((snap) => {
    window.__testCalls = []
    window.__outgoingRequests = [{ id: 'missed-menu', clipboard: false, fileName: '菜单文件.txt', bytes: 123 }]
    // Only an unrelated snapshot arrives. It must not acknowledge a missing card.
    window.__emit('snapshot', snap)
  }, lateSnapshot)
  await missed.waitForTimeout(100)
  assert.equal(
    await missed.evaluate(() => window.__testCalls.filter((c) => c.cmd === 'acknowledge_receive_view').length),
    0,
    'An empty view must not cancel the request watchdog'
  )
  await missed.getByText('菜单文件.txt', { exact: false }).waitFor()
  await missed.waitForFunction(() =>
    window.__testCalls.some((c) => c.cmd === 'acknowledge_receive_view' && c.args.ids.includes('outgoing:missed-menu'))
  )
  // Failure to show the native window must leave this request unacknowledged.
  await missed.evaluate(() => {
    window.__testCalls = []
    window.__showFailure = true
    window.__outgoingRequests = [{ id: 'show-failed', clipboard: false, fileName: '第二个文件.txt', bytes: 321 }]
    window.__emit('outgoing-requests', window.__outgoingRequests)
  })
  await missed.getByText('第二个文件.txt', { exact: false }).waitFor()
  await missed.waitForTimeout(100)
  assert.equal(
    await missed.evaluate(() => window.__testCalls.filter((c) => c.cmd === 'acknowledge_receive_view').length),
    0,
    'A failed show must not acknowledge display'
  )
  await missed.evaluate(() => {
    window.__showFailure = false
    window.__outgoingRequests = [{ id: 'visible-again', clipboard: false, fileName: '恢复的文件.txt', bytes: 456 }]
    document.dispatchEvent(new Event('visibilitychange'))
  })
  await missed.getByText('恢复的文件.txt', { exact: false }).waitFor()
  await missed.waitForFunction(() =>
    window.__testCalls.some((c) => c.cmd === 'acknowledge_receive_view' && c.args.ids.includes('outgoing:visible-again'))
  )
  await missed.close()
  const recovery = await browser.newPage({ viewport: { width: 540, height: 690 } })
  await recovery.addInitScript(installMock, { snap: lateSnapshot, label: 'receive', platform: 'Win32', now: epoch })
  await recovery.addInitScript(() => {
    window.__clipboardReceipts = [{ id: 50, kind: 'text', preview: '重复内容', text: '重复内容', peer: 'Mac' }]
    window.__holdReceiptRead = true
  })
  await recovery.goto('http://127.0.0.1:1420/?window=receive')
  await recovery.waitForFunction(() => typeof window.__finishReceiptRead === 'function')
  await recovery.evaluate(() => {
    window.__emit('clipboard-popup', { id: 51, kind: 'text', preview: '重复内容', text: '重复内容', peer: 'Mac' })
    window.__finishReceiptRead()
  })
  await recovery.waitForFunction(() => document.querySelectorAll('.popup-card--clip').length === 2)
  await recovery.waitForFunction(() =>
    window.__testCalls.some(
      (c) => c.cmd === 'acknowledge_clipboard_receipts' && c.args.ids.includes(50) && c.args.ids.includes(51)
    )
  )
  await recovery.evaluate(() => {
    window.__emit('clipboard-popup', { id: 51, kind: 'text', preview: '重复内容', text: '重复内容', peer: 'Mac' })
  })
  assert.equal(await recovery.locator('.popup-card--clip').count(), 2, 'Only identical receipt IDs are deduplicated')
  // Simulate a lost event followed by a hidden WebView becoming visible.
  await recovery.evaluate(() => {
    window.__holdReceiptRead = false
    window.__clipboardReceipts = [{ id: 52, kind: 'image', preview: '640×480 图片', peer: 'Mac' }]
    document.dispatchEvent(new Event('visibilitychange'))
  })
  await recovery.getByText('收到剪贴板图片', { exact: true }).waitFor()
  await recovery.locator('.popup-card--clip').first().click()
  // A delayed hydration of an already dismissed receipt must not resurrect it.
  await recovery.evaluate(() => {
    window.__clipboardReceipts = [{ id: 52, kind: 'image', preview: '640×480 图片', peer: 'Mac' }]
    document.dispatchEvent(new Event('visibilitychange'))
  })
  assert.equal(await recovery.getByText('收到剪贴板图片', { exact: true }).count(), 0)
  await recovery.waitForFunction(() => document.querySelectorAll('.popup-card--clip').length === 0, null, {
    timeout: 10000,
  })
  await recovery.waitForFunction(() => window.__testCalls.at(-1)?.cmd === 'plugin:window|hide')
  await recovery.close()
  console.log(
    `${engine}: popup layout, offers, late receipt hydration, visibility recovery, identical recopy, deduplication and dismissal passed`
  )
} finally {
  await browser.close()
}

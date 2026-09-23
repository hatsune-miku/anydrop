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
    assert.deepEqual(errors, [])
    await page.close()
  }
  console.log(
    `${engine}: popup transparency, shadow gutters, receipt title, multiple/single/no peer, error and cancellation passed`
  )
} finally {
  await browser.close()
}

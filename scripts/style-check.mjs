import assert from 'node:assert/strict'
import { mkdir, writeFile } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { PNG } from 'pngjs'

import { epoch, installMock, snapshot } from '../tests/visual/fixture.mjs'

const engine = process.env.VISUAL_ENGINE || 'chromium'
const browser = await { chromium, webkit }[engine].launch()
const base = process.env.VISUAL_URL || 'http://127.0.0.1:1420'
const output = 'test-results/style-fixes'
const results = []
await mkdir(output, { recursive: true })
try {
  for (const height of [770, 580]) {
    const snap = snapshot(false)
    snap.peers = Array.from({ length: 40 }, (_, index) => ({ ...snap.peers[0], name: `device:${index}` }))
    snap.transfers = Array.from({ length: 40 }, (_, index) => ({ ...snap.transfers[0], key: String(index) }))
    snap.logs = Array.from({ length: 60 }, (_, index) => `[10:00:00.000] Log ${index}`)
    const page = await browser.newPage({ viewport: { width: 1060, height } })
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.addInitScript(installMock, { snap, label: 'main', platform: 'MacIntel', now: epoch })
    await page.goto(base)
    await page.locator('.settings-pane').waitFor()
    await page.mouse.move(0, 0)
    for (const selector of ['.device-list', '.transfer-list', '.log-list', '.settings-scroll']) {
      const container = page.locator(selector)
      const original = await container.evaluate((element) => ({
        clientHeight: element.clientHeight,
        scrollHeight: element.scrollHeight,
      }))
      if (original.scrollHeight <= original.clientHeight) continue
      // Only recolor the native thumb for pixel detection; preserve production geometry and track styling.
      await container.evaluate((element) => element.style.setProperty('--cake-scrollbar-thumb', '#0000ff'))
      for (const end of [false, true]) {
        await container.evaluate((element, bottom) => (element.scrollTop = bottom ? element.scrollHeight : 0), end)
        const geometry = await container.evaluate((element) => ({
          clientHeight: element.clientHeight,
          scrollHeight: element.scrollHeight,
          scrollTop: element.scrollTop,
        }))
        assert.equal(geometry.clientHeight, original.clientHeight)
        assert.equal(geometry.scrollHeight, original.scrollHeight)
        assert.equal(geometry.scrollTop, end ? geometry.scrollHeight - geometry.clientHeight : 0)
        const bytes = await container.screenshot({
          path: `${output}/${engine}-${height}-${selector.slice(1)}-${end ? 'end' : 'start'}.png`,
        })
        // macOS Chromium headless suppresses scrollbar painting. WebKit paints the actual native thumb.
        if (engine === 'webkit') {
          const png = PNG.sync.read(bytes)
          const rows = []
          for (let y = 0; y < png.height; y++)
            for (let x = png.width - 8; x < png.width; x++) {
              const offset = (y * png.width + x) * 4
              if (png.data[offset] < 20 && png.data[offset + 1] < 20 && png.data[offset + 2] > 220) rows.push(y)
            }
          assert(rows.length > 0, `${selector}: native thumb must be visible`)
          const first = Math.min(...rows)
          const last = Math.max(...rows)
          assert(first >= (selector === '.settings-scroll' ? 24 : 4), `${selector}: top inset`)
          assert(last < png.height - (selector === '.settings-scroll' ? 4 : 24), `${selector}: bottom inset`)
          results.push({ height, selector, end, first, last, ...geometry })
        }
      }
      await container.evaluate((element) => element.style.removeProperty('--cake-scrollbar-thumb'))
    }
    await page.getByRole('button', { name: '更多设置', exact: true }).click()
    await page.getByLabel('主题色与明暗', { exact: true }).click()
    const dialog = page.getByRole('dialog', { name: '主题设置', exact: true })
    const original = await dialog.boundingBox()
    for (const name of ['葡萄', '自定义', '浅色文字', '深色文字', '粉']) {
      const target = dialog.getByRole('radio', { name, exact: true })
      await target.scrollIntoViewIfNeeded()
      const bounds = await target.boundingBox()
      await page.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
      assert(await target.isChecked(), `${engine}/${height}/${name}: first click`)
      assert.deepEqual(await dialog.boundingBox(), original, `${engine}/${height}/${name}: stable dialog`)
    }
    await dialog.evaluate((element) => element.focus())
    assert.equal(await dialog.evaluate((element) => getComputedStyle(element).outlineStyle), 'none')
    await page.screenshot({ path: `${output}/${engine}-${height}-theme.png`, animations: 'disabled' })
    assert.deepEqual(errors, [])
    await page.close()
  }
} finally {
  await browser.close()
}
await writeFile(`${output}/${engine}-report.json`, JSON.stringify(results, null, 2) + '\n')
console.log(
  `${engine}: scrollbar endpoints, unchanged scroll range, single-click theme choices and dialog focus passed`
)

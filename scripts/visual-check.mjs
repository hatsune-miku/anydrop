import assert from 'node:assert/strict'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

import { epoch, installMock, snapshot } from '../tests/visual/fixture.mjs'

const require = createRequire(import.meta.url)
const { chromium, webkit } = require('playwright')
const { PNG } = require('pngjs')
const { default: pixelmatch } = await import(pathToFileURL(require.resolve('pixelmatch')))
const record = process.argv.includes('--record')
const migration = process.argv.includes('--migration')
assert(!(record && migration), 'Original migration baselines must not be overwritten')
const engine = process.env.VISUAL_ENGINE || 'chromium'
const scenarios = (process.env.VISUAL_SCENARIOS || 'main,mac,compact,empty,send,hover,hint,receive,preview').split(',')
const base = process.env.VISUAL_URL || 'http://127.0.0.1:1420'
const baseline = path.resolve(
  process.env.VISUAL_BASELINE || (migration ? 'tests/visual/before-cakeui' : 'tests/visual/baseline')
)
const output = path.resolve('test-results/visual', engine + (migration ? '-migration' : ''))
await mkdir(baseline, { recursive: true })
await mkdir(output, { recursive: true })
const browser = await { chromium, webkit }[engine].launch({
  headless: true,
  ...(process.env.VISUAL_BROWSER ? { executablePath: process.env.VISUAL_BROWSER } : {}),
})
const results = []
try {
  for (const mode of ['light', 'dark']) {
    for (const scenario of scenarios) {
      const label = ['receive', 'preview'].includes(scenario) ? scenario : 'main'
      const snap = snapshot(mode === 'dark')
      if (scenario === 'empty') {
        snap.peers = []
        snap.transfers = []
      }
      if (scenario === 'receive')
        snap.transfers.unshift({
          ...snap.transfers[0],
          key: '104',
          direction: 'incoming',
          status: 1,
          fileName: '设计素材.zip',
        })
      const viewport =
        label === 'receive'
          ? { width: 400, height: 500 }
          : label === 'preview'
            ? { width: 760, height: 580 }
            : scenario === 'compact'
              ? { width: 1020, height: 580 }
              : { width: 1060, height: 770 }
      const context = await browser.newContext({
        viewport,
        colorScheme: mode,
        deviceScaleFactor: 1,
        locale: 'zh-CN',
        timezoneId: 'Asia/Shanghai',
      })
      await context.addInitScript(installMock, {
        snap,
        label,
        platform: scenario === 'mac' ? 'MacIntel' : 'Win32',
        now: epoch,
      })
      const page = await context.newPage()
      const errors = []
      page.on('pageerror', (error) => errors.push(error.message))
      await page.goto(`${base}/?window=${label}`)
      // Isolate component migration from the separately tested, newly added desktop settings.
      // The default regression run always renders the complete production UI.
      if (migration) await page.addStyleTag({ content: '.desktop-settings { display: none; }' })
      await page.waitForSelector(
        label === 'main' ? '.settings-pane' : label === 'receive' ? '.popup-card' : '.preview-media'
      )
      await page.waitForTimeout(700)
      await page.mouse.move(0, 0)
      if (scenario === 'send') await page.getByRole('button', { name: '选择文件', exact: true }).click()
      if (scenario === 'hover') await page.getByRole('button', { name: '选择文件', exact: true }).hover()
      if (scenario === 'hint') await page.locator('.section-heading .field-hint').hover()
      await page.waitForTimeout(300)
      const name = `${engine}-${mode}-${scenario}.png`
      const current = await page.screenshot({ animations: 'disabled' })
      if (record) {
        await writeFile(path.join(baseline, name), current)
        results.push({ name, recorded: true })
      } else {
        await writeFile(path.join(output, name), current)
        const a = PNG.sync.read(await readFile(path.join(baseline, name)))
        const b = PNG.sync.read(current)
        assert.equal(a.width, b.width)
        assert.equal(a.height, b.height)
        const diff = new PNG({ width: a.width, height: a.height })
        const changed = pixelmatch(a.data, b.data, diff.data, a.width, a.height, { threshold: 0.02 })
        let rawChangedPixels = 0
        for (let i = 0; i < a.data.length; i += 4) {
          if (
            a.data[i] !== b.data[i] ||
            a.data[i + 1] !== b.data[i + 1] ||
            a.data[i + 2] !== b.data[i + 2] ||
            a.data[i + 3] !== b.data[i + 3]
          )
            rawChangedPixels++
        }
        await writeFile(path.join(output, name.replace('.png', '-diff.png')), PNG.sync.write(diff))
        results.push({ name, changedPixels: changed, rawChangedPixels, ratio: changed / (a.width * a.height) })
      }
      if (scenario === 'send') {
        const calls = await page.evaluate(() => window.__testCalls)
        assert(!calls.some((c) => c.cmd === 'send_paths'), 'Choosing files must not send before confirmation')
        await page.getByRole('button', { name: '确认发送', exact: true }).click()
        assert(
          (await page.evaluate(() => window.__testCalls)).some((c) => c.cmd === 'send_paths'),
          'Confirm must invoke send_paths'
        )
      }
      if (scenario === 'receive') {
        await page.getByRole('button', { name: '接收', exact: true }).click()
        assert(
          (await page.evaluate(() => window.__testCalls)).some((c) => c.cmd === 'accept_transfer'),
          'Receive must invoke accept_transfer'
        )
      }
      assert.deepEqual(errors, [], 'Browser exceptions')
      await context.close()
    }
  }
} finally {
  await browser.close()
}
await writeFile(path.join(output, 'report.json'), JSON.stringify(results, null, 2) + '\n')
console.log(JSON.stringify(results, null, 2))
if (!record && results.some((r) => r.ratio > Number(process.env.VISUAL_MAX_RATIO || '0.0001'))) process.exitCode = 1

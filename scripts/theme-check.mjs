import assert from 'node:assert/strict'
import { mkdir, writeFile } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'

import { createThemeConfig } from '@a1knla/cakeui/theme'

import { epoch, installMock, snapshot } from '../tests/visual/fixture.mjs'

const engine = process.env.VISUAL_ENGINE || 'chromium'
const browser = await { chromium, webkit }[engine].launch()
const output = 'test-results/theme-borders'
await mkdir(output, { recursive: true })
const results = []
try {
  for (const scheme of ['blue', 'pink', 'gold', 'dousha', 'dousha-classic', 'grape', 'custom']) {
    for (const mode of ['light', 'dark']) {
      const snap = snapshot(mode === 'dark')
      snap.settings.appearanceTheme = scheme
      snap.settings.appearanceConfig = createThemeConfig({ scheme, mode, customColor: '#524396' })
      const page = await browser.newPage({ viewport: { width: 1060, height: 770 } })
      const errors = []
      page.on('pageerror', (error) => errors.push(error.message))
      await page.addInitScript(installMock, { snap, label: 'main', platform: 'MacIntel', now: epoch })
      await page.goto(process.env.VISUAL_URL || 'http://127.0.0.1:1420')
      await page.locator('.settings-pane').waitFor()
      const cards = await page.locator('.cake-card').evaluateAll((nodes) =>
        nodes.map((node) => {
          const css = getComputedStyle(node)
          return { background: css.backgroundColor, shadow: css.boxShadow, border: css.borderTopWidth }
        })
      )
      assert.equal(cards.length, 5)
      for (const card of cards) {
        assert.equal(card.shadow, 'none', `${scheme}/${mode}: cards must not draw an outline`)
        assert.equal(card.border, '0px', `${scheme}/${mode}: cards must not draw a border`)
        assert.notEqual(card.background, 'rgba(0, 0, 0, 0)', 'Cards use a surface fill')
        results.push({ scheme, mode, ...card })
      }
      await page.screenshot({ path: `${output}/${engine}-${scheme}-${mode}.png`, animations: 'disabled' })
      assert.deepEqual(errors, [])
      await page.close()
    }
  }
} finally {
  await browser.close()
}
await writeFile(`${output}/${engine}-report.json`, JSON.stringify(results, null, 2) + '\n')
console.log(`${engine}: all seven schemes use borderless cards in both modes`)

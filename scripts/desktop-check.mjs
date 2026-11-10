import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'

import { epoch, installMock, snapshot } from '../tests/visual/fixture.mjs'

const engine = process.env.VISUAL_ENGINE || 'chromium'
const browser = await { chromium, webkit }[engine].launch({
  ...(process.env.VISUAL_BROWSER ? { executablePath: process.env.VISUAL_BROWSER } : {}),
})
try {
  for (const hasTransfers of [true, false]) {
    const mode = hasTransfers ? 'light' : 'dark'
    const shortcutLabel = hasTransfers ? 'Ctrl + Alt + V' : 'Ctrl + Option + V'
    const snap = snapshot(mode === 'dark')
    if (!hasTransfers) snap.transfers = []
    const page = await browser.newPage({ viewport: { width: 1060, height: hasTransfers ? 770 : 580 } })
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.addInitScript(installMock, {
      snap,
      label: 'main',
      platform: hasTransfers ? 'Win32' : 'MacIntel',
      now: epoch,
      updaterConfigured: true,
    })
    await page.goto(process.env.VISUAL_URL || 'http://127.0.0.1:1420')
    const moreButton = page.getByRole('button', { name: '更多设置', exact: true })
    const moreDialog = page.getByRole('dialog', { name: '更多设置', exact: true })
    const updateDialog = page.getByRole('dialog', { name: '更新到 0.1.3', exact: true })
    const autostart = page.getByRole('switch', { name: '开机自启', exact: true })
    assert(!(await autostart.isVisible()), 'Desktop extras stay out of the primary settings pane')
    await moreButton.click()
    await moreDialog.waitFor()
    const theme = page.getByLabel('主题色与明暗', { exact: true })
    const density = page.getByLabel('界面密度', { exact: true })
    const themeDialog = page.getByRole('dialog', { name: '主题设置', exact: true })
    const applyTheme = themeDialog.getByRole('button', { name: '应用', exact: true })
    await mkdir('test-results/desktop', { recursive: true })
    await theme.click()
    const callsBeforePreview = await page.evaluate(
      () => window.__testCalls.filter((c) => c.cmd === 'save_settings').length
    )
    for (const [id, name] of [
      ['blue', '蓝'],
      ['pink', '粉'],
      ['gold', '金'],
      ['dousha', '豆沙'],
      ['dousha-classic', '绿豆沙'],
      ['grape', '葡萄'],
    ]) {
      await themeDialog.getByRole('radio', { name, exact: true }).check()
      await page.waitForFunction((scheme) => document.querySelector('.app-theme')?.dataset.theme === scheme, id)
    }
    await themeDialog.getByRole('radio', { name: '深色', exact: true }).check()
    await themeDialog.getByRole('radio', { name: '浅色文字', exact: true }).check()
    await page.screenshot({ path: `test-results/desktop/${engine}-theme-grape-${mode}.png`, animations: 'disabled' })
    await page.keyboard.press('Escape')
    await themeDialog.waitFor({ state: 'hidden' })
    assert(await moreDialog.isVisible(), 'Esc cancels only the theme editor')
    assert.equal(await page.locator('.app-theme').getAttribute('data-theme'), 'pink')
    assert.equal(
      await page.evaluate(() => window.__testCalls.filter((c) => c.cmd === 'save_settings').length),
      callsBeforePreview,
      'Preview never writes native settings'
    )
    assert(await theme.evaluate((button) => button === document.activeElement), 'Theme cancel restores entry focus')

    await theme.click()
    await themeDialog.getByRole('radio', { name: '葡萄', exact: true }).check()
    await themeDialog.getByRole('radio', { name: '深色', exact: true }).check()
    await themeDialog.getByRole('radio', { name: '浅色文字', exact: true }).check()
    await page.evaluate(() => {
      window.__pauseSettingsSave = true
    })
    await applyTheme.click()
    await page.waitForFunction(() => !!window.__finishSettingsSave)
    await themeDialog.waitFor({ state: 'hidden' })
    assert.equal(await theme.getAttribute('aria-disabled'), 'true', 'Pending native save prevents reopening')
    assert(!(await theme.evaluate((button) => button.disabled)), 'Pending native save keeps the entry focusable')
    assert(
      await theme.evaluate((button) => button === document.activeElement),
      'Apply restores entry focus during native save'
    )
    await page.evaluate(() => {
      window.__pauseSettingsSave = false
      window.__finishSettingsSave()
    })
    await page.waitForFunction(
      () => document.querySelector('#appearance-theme')?.getAttribute('aria-disabled') !== 'true'
    )
    await theme.click()
    await themeDialog.getByRole('radio', { name: '自定义', exact: true }).check()
    const color = themeDialog.getByLabel('HEX / RGB', { exact: true })
    await color.fill('not-a-color')
    assert(await applyTheme.isDisabled(), 'Invalid custom color cannot be applied')
    await color.fill('rgb(82, 67, 150)')
    await themeDialog.getByRole('radio', { name: '跟随系统', exact: true }).check()
    await themeDialog.getByRole('radio', { name: '深色文字', exact: true }).check()
    await applyTheme.click()
    await density.selectOption('comfortable')
    await page.emulateMedia({ colorScheme: 'dark' })
    await page.waitForFunction(() => document.documentElement.classList.contains('dark'))
    const darkBackground = await page.locator('.app-theme').evaluate((node) => getComputedStyle(node).backgroundColor)
    await page.emulateMedia({ colorScheme: 'light' })
    await page.waitForFunction(() => !document.documentElement.classList.contains('dark'))
    const lightBackground = await page.locator('.app-theme').evaluate((node) => getComputedStyle(node).backgroundColor)
    assert.notEqual(darkBackground, lightBackground, 'System mode changes rendered theme colors')
    const saved = await page.evaluate(
      () => window.__testCalls.filter((call) => call.cmd === 'save_settings').at(-1).args.settings
    )
    assert.equal(saved.appearanceTheme, 'custom')
    assert.equal(saved.appearanceConfig.customColor, '#524396')
    assert.deepEqual(saved.appearanceConfig.options.grape, { mode: 'dark', foreground: 'light' })
    assert.deepEqual(saved.appearanceConfig.options.custom, { mode: 'system', foreground: 'dark' })
    assert.equal(saved.appearanceDensity, 'comfortable')
    assert.equal(saved.darkMode, null)
    for (const label of ['main', 'receive', 'preview']) {
      const auxiliary = await browser.newPage({ viewport: { width: 540, height: 690 } })
      await auxiliary.addInitScript(installMock, {
        snap: { ...snap, settings: saved },
        label,
        platform: 'MacIntel',
        now: epoch,
      })
      await auxiliary.goto(`http://127.0.0.1:1420/?window=${label}`)
      await auxiliary.waitForFunction(() => document.querySelector('.app-theme')?.dataset.theme === 'custom')
      assert.equal(await auxiliary.locator('.app-theme').getAttribute('data-density'), 'comfortable')
      await auxiliary.evaluate((next) => window.__emit('snapshot', next), {
        ...snap,
        settings: { ...saved, appearanceTheme: 'blue', darkMode: true },
      })
      await auxiliary.waitForFunction(
        () =>
          document.querySelector('.app-theme')?.dataset.theme === 'blue' &&
          document.querySelector('.app-theme')?.dataset.mode === 'dark'
      )
      await auxiliary.close()
    }
    await theme.click()
    await themeDialog.getByRole('radio', { name: '葡萄', exact: true }).check()
    assert(
      await themeDialog.getByRole('radio', { name: '深色', exact: true }).isChecked(),
      'Each preset remembers its mode'
    )
    assert(
      await themeDialog.getByRole('radio', { name: '浅色文字', exact: true }).isChecked(),
      'Each preset remembers its foreground'
    )
    await themeDialog.getByRole('radio', { name: '自定义', exact: true }).check()
    assert.equal(await color.inputValue(), '#524396', 'Preset switching keeps the custom color')
    await page.screenshot({ path: `test-results/desktop/${engine}-theme-custom-${mode}.png`, animations: 'disabled' })
    const themeBounds = await themeDialog.boundingBox()
    assert(
      themeBounds.y >= 0 && themeBounds.y + themeBounds.height <= page.viewportSize().height,
      'Theme editor fits the window'
    )
    await themeDialog.getByRole('button', { name: '重置当前配色', exact: true }).click()
    await themeDialog.getByRole('button', { name: '取消', exact: true }).last().click()
    await theme.click()
    assert.equal(await color.inputValue(), '#524396', 'Cancelling reset retains the applied custom color')
    await themeDialog.getByRole('radio', { name: '粉', exact: true }).check()
    await themeDialog.getByRole('radio', { name: mode === 'dark' ? '深色' : '浅色', exact: true }).check()
    await applyTheme.click()
    await density.selectOption('compact')
    await mkdir('test-results/desktop', { recursive: true })
    await page.screenshot({ path: `test-results/desktop/${engine}-more-settings-${mode}.png`, animations: 'disabled' })
    const bounds = await moreDialog.boundingBox()
    assert(bounds.y >= 0 && bounds.y + bounds.height <= page.viewportSize().height, 'Dialog fits the window')
    await autostart.check()
    assert(await autostart.isChecked())
    assert((await page.evaluate(() => window.__testCalls)).some((c) => c.cmd === 'set_autostart' && c.args.enabled))
    const autoReceive = page.getByRole('switch', { name: '自动接收文件', exact: true })
    const contextMenu = page.getByRole('switch', { name: '注册文件右键菜单', exact: true })
    assert(!(await autoReceive.isChecked()), 'File auto-accept defaults off')
    assert(await contextMenu.isChecked(), 'Context menus default on')
    await autoReceive.check()
    await contextMenu.uncheck()
    assert(!(await page.getByRole('button', { name: '重新注册', exact: true }).count()))
    await contextMenu.check()
    await page.getByRole('button', { name: '重新注册', exact: true }).click()
    assert(await autoReceive.isChecked(), 'Other preferences survive context-menu changes')
    const location = page.getByLabel('菜单位置', { exact: true })
    if (hasTransfers) {
      assert.equal(await location.count(), 0, 'Menu location is macOS-only')
    } else {
      assert.equal(await location.inputValue(), 'services', 'An existing service keeps its location')
      await location.selectOption('finder')
      await page.getByRole('button', { name: '访达扩展设置', exact: true }).waitFor()
      await contextMenu.uncheck()
      assert.equal(await location.inputValue(), 'finder', 'Disabling preserves the chosen location')
      await contextMenu.check()
      await page.getByRole('button', { name: '重新注册', exact: true }).click()
      assert.equal(await location.inputValue(), 'finder', 'Repair preserves the chosen location')
      await page.screenshot({ path: `test-results/desktop/${engine}-finder-settings.png`, animations: 'disabled' })
      await location.selectOption('services')
      await page.getByRole('button', { name: '访达扩展设置', exact: true }).waitFor({ state: 'hidden' })
      assert.equal(await page.getByRole('button', { name: '访达扩展设置', exact: true }).count(), 0)
    }
    await page.getByRole('button', { name: '录制', exact: true }).click()
    const shortcut = page.getByLabel('发送当前剪贴板', { exact: true })
    await shortcut.press('Control+Alt+KeyV')
    await page.waitForFunction(() => window.__testCalls.some((c) => c.cmd === 'set_clipboard_shortcut'))
    assert.equal(await shortcut.inputValue(), shortcutLabel)
    assert.equal(
      (await page.evaluate(() => window.__testCalls.filter((c) => c.cmd === 'set_clipboard_shortcut').at(-1))).args
        .shortcut,
      'Control+Alt+KeyV',
      'Readable shortcut labels preserve the native binding format'
    )
    await page.getByRole('button', { name: '录制', exact: true }).click()
    await moreDialog.getByRole('button', { name: '取消', exact: true }).click()
    await page.getByRole('button', { name: '录制', exact: true }).waitFor()
    assert.equal(await shortcut.inputValue(), shortcutLabel, 'Cancelling capture keeps the previous binding')
    await page.getByRole('button', { name: '录制', exact: true }).click()
    await shortcut.press('Escape')
    assert(await moreDialog.isVisible(), 'Escape cancels shortcut capture before closing settings')
    await page.getByRole('button', { name: '录制', exact: true }).click()
    await page.getByRole('button', { name: '关闭更多设置', exact: true }).click()
    await moreDialog.waitFor({ state: 'hidden' })
    assert(await moreButton.evaluate((button) => button === document.activeElement), 'Closing restores trigger focus')
    await page.waitForFunction(
      () => window.__testCalls.filter((c) => c.cmd === 'set_shortcut_recording').at(-1)?.args.recording === false
    )
    await moreButton.click()
    assert(await autostart.isChecked(), 'Autostart persists across dialog reopening')
    assert.equal(await shortcut.inputValue(), shortcutLabel, 'Shortcut persists across dialog reopening')
    assert(
      !(await page.evaluate(() => window.__testCalls)).some((c) => c.cmd === 'send_clipboard_now'),
      'Recording must never transmit clipboard data'
    )
    await page.evaluate(() => {
      window.__shortcutFailure = true
    })
    await page.getByRole('button', { name: '录制', exact: true }).click()
    await shortcut.press('Control+Alt+KeyB')
    await page.getByRole('alert').filter({ hasText: '快捷键已被占用' }).waitFor()
    assert.equal(await shortcut.inputValue(), shortcutLabel, 'Failed registration preserves previous binding')
    await page.evaluate(() => {
      window.__shortcutFailure = false
    })
    await page.getByRole('button', { name: '清除', exact: true }).click()
    await page.waitForFunction(() => document.querySelector('#clipboard-shortcut').value === '')
    await page.getByRole('button', { name: '检查更新', exact: true }).click()
    await page.getByRole('button', { name: '新版本 0.1.3', exact: true }).click()
    await page.evaluate(() => {
      window.__updateDownloadFailure = true
    })
    await page.getByRole('button', { name: '下载更新', exact: true }).click()
    await updateDialog.getByRole('status').filter({ hasText: '签名校验失败' }).waitFor()
    assert.equal(await page.getByRole('button', { name: '安装并重启', exact: true }).count(), 0)
    await page.evaluate(() => {
      window.__updateDownloadFailure = false
      window.__holdUpdateDownload = true
    })
    await page.getByRole('button', { name: '下载更新', exact: true }).click()
    await page.getByRole('button', { name: '下载中…', exact: true }).waitFor()
    await page.keyboard.press('Escape')
    await updateDialog.waitFor({ state: 'hidden' })
    assert(await moreDialog.isVisible(), 'Closing update details leaves more settings open')
    await page.getByRole('button', { name: '完成', exact: true }).click()
    await moreDialog.waitFor({ state: 'hidden' })
    await moreButton.click()
    await page.getByRole('button', { name: '新版本 0.1.3', exact: true }).click()
    await page.getByRole('button', { name: '下载中…', exact: true }).waitFor()
    await page.evaluate(() => window.__finishUpdateDownload())
    const install = page.getByRole('button', { name: '安装并重启', exact: true })
    await install.waitFor()
    assert.equal(await install.isDisabled(), hasTransfers)
    await mkdir('test-results/desktop', { recursive: true })
    await page.screenshot({
      path: `test-results/desktop/${engine}-update-${hasTransfers ? 'blocked' : 'ready'}.png`,
      animations: 'disabled',
    })
    if (!hasTransfers) {
      await install.click()
      assert((await page.evaluate(() => window.__testCalls)).some((c) => c.cmd === 'install_app_update'))
    } else {
      assert(!(await page.evaluate(() => window.__testCalls)).some((c) => c.cmd === 'install_app_update'))
      await page.keyboard.press('Escape')
    }
    assert.deepEqual(errors, [])
    await page.close()
  }
  console.log(
    'Desktop UI checks passed: six theme presets, custom color validation and persistence, preview/cancel/reset, per-scheme memory, system mode, async save focus, three-window sync, desktop preferences, shortcuts, nested updates and transfer guard.'
  )
} finally {
  await browser.close()
}

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
    const snap = snapshot(mode === 'dark')
    if (!hasTransfers) snap.transfers = []
    const page = await browser.newPage({ viewport: { width: 1060, height: hasTransfers ? 770 : 580 } })
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.addInitScript(installMock, {
      snap,
      label: 'main',
      platform: 'Win32',
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
    await page.getByRole('button', { name: '录制', exact: true }).click()
    const shortcut = page.getByLabel('发送当前剪贴板', { exact: true })
    await shortcut.press('Control+Alt+KeyV')
    await page.waitForFunction(() => window.__testCalls.some((c) => c.cmd === 'set_clipboard_shortcut'))
    assert.equal(await shortcut.inputValue(), 'Ctrl + Alt + V')
    assert.equal(
      (await page.evaluate(() => window.__testCalls.filter((c) => c.cmd === 'set_clipboard_shortcut').at(-1))).args
        .shortcut,
      'Control+Alt+KeyV',
      'Readable shortcut labels preserve the native binding format'
    )
    await page.getByRole('button', { name: '录制', exact: true }).click()
    await moreDialog.getByRole('button', { name: '取消', exact: true }).click()
    await page.getByRole('button', { name: '录制', exact: true }).waitFor()
    assert.equal(await shortcut.inputValue(), 'Ctrl + Alt + V', 'Cancelling capture keeps the previous binding')
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
    assert.equal(await shortcut.inputValue(), 'Ctrl + Alt + V', 'Shortcut persists across dialog reopening')
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
    assert.equal(await shortcut.inputValue(), 'Ctrl + Alt + V', 'Failed registration preserves previous binding')
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
    'Desktop UI checks passed: more settings in both themes, focus restoration, autostart, shortcut capture/cancel/conflict/clear, no accidental sends, nested update details, download persistence and transfer guard.'
  )
} finally {
  await browser.close()
}

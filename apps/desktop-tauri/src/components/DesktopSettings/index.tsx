import { useEffect, useRef, useState } from 'react'
import type { KeyboardEvent } from 'react'

import { Button, CheckBox, Dialog, ProgressBar, TextBox } from '@a1knla/cakeui'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import './index.scss'

import { formatBytes, isTauriRuntime } from '../../types'

interface Preferences {
  autostart: boolean
  clipboardShortcut: string
  shortcutRegistered: boolean
  shortcutError: string | null
  updaterConfigured: boolean
}
interface UpdateInfo {
  version: string
  notes: string
}
type UpdatePhase = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'installing'

export function DesktopSettings({ hasTransfers, disabled = false }: { hasTransfers: boolean; disabled?: boolean }) {
  const [showMore, setShowMore] = useState(false)
  const [preferences, setPreferences] = useState<Preferences | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [recording, setRecording] = useState(false)
  const recordingAttempt = useRef(0)
  const input = useRef<HTMLInputElement>(null)
  const [phase, setPhase] = useState<UpdatePhase>('idle')
  const [update, setUpdate] = useState<UpdateInfo | null>(null)
  const [message, setMessage] = useState('')
  const [progress, setProgress] = useState({ downloaded: 0, total: undefined as number | undefined })
  const [showUpdate, setShowUpdate] = useState(false)
  const operation = useRef(false)

  async function refresh() {
    try {
      setPreferences(await invoke<Preferences>('get_desktop_preferences'))
      setError('')
    } catch (e) {
      setError(String(e))
    }
  }

  useEffect(() => {
    if (!isTauriRuntime()) return
    void refresh()
    const preferencesChanged = listen('desktop-preferences-changed', () => void refresh())
    const progressChanged = listen<{ downloaded: number; total?: number }>('app-update-progress', ({ payload }) => {
      setProgress({ downloaded: payload.downloaded, total: payload.total })
    })
    return () => {
      void preferencesChanged.then((stop) => stop())
      void progressChanged.then((stop) => stop())
      void invoke('set_shortcut_recording', { recording: false })
    }
  }, [])

  async function checkUpdate() {
    if (operation.current) return
    operation.current = true
    setPhase('checking')
    setMessage('正在检查更新…')
    try {
      const next = await invoke<UpdateInfo | null>('check_app_update')
      setUpdate(next)
      if (!next) setShowUpdate(false)
      setPhase(next ? 'available' : 'idle')
      setMessage(next ? `发现新版本 ${next.version}` : '已是最新版本')
    } catch (e) {
      setPhase('idle')
      setMessage(String(e))
    } finally {
      operation.current = false
    }
  }

  useEffect(() => {
    if (!preferences?.updaterConfigured || import.meta.env.DEV) return
    const timer = setTimeout(() => void checkUpdate(), 10_000)
    return () => clearTimeout(timer)
  }, [preferences?.updaterConfigured])

  async function setAutostart(enabled: boolean) {
    setBusy(true)
    setError('')
    try {
      setPreferences(await invoke<Preferences>('set_autostart', { enabled }))
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  async function stopRecording() {
    recordingAttempt.current += 1
    setRecording(false)
    await invoke('set_shortcut_recording', { recording: false })
  }

  async function startRecording() {
    const attempt = ++recordingAttempt.current
    setError('')
    try {
      await invoke('set_shortcut_recording', { recording: true })
      if (attempt !== recordingAttempt.current) return
      setRecording(true)
      input.current?.focus()
    } catch (e) {
      setError(String(e))
    }
  }

  function setMoreOpen(open: boolean) {
    if (phase === 'installing') return
    setShowMore(open)
    if (!open) {
      setShowUpdate(false)
      void stopRecording().catch((e) => setError(String(e)))
    }
  }

  async function saveShortcut(shortcut: string) {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      setPreferences(await invoke<Preferences>('set_clipboard_shortcut', { shortcut }))
    } catch (e) {
      await refresh()
      setError(String(e))
    } finally {
      await stopRecording().catch(() => {})
      setBusy(false)
    }
  }

  function recordKey(event: KeyboardEvent<HTMLInputElement>) {
    if (!recording || event.nativeEvent.isComposing) return
    if (event.key === 'Tab') {
      void stopRecording()
      return
    }
    event.preventDefault()
    event.stopPropagation()
    if (event.key === 'Escape') {
      void stopRecording()
      return
    }
    if (event.repeat || ['Control', 'Shift', 'Alt', 'Meta'].includes(event.key)) return
    if (!event.ctrlKey && !event.altKey && !event.metaKey) {
      setError('请同时按住 Ctrl、Alt 或 Command / Win')
      return
    }
    const modifiers = [
      event.ctrlKey && 'Control',
      event.altKey && 'Alt',
      event.shiftKey && 'Shift',
      event.metaKey && 'Super',
    ].filter(Boolean)
    void saveShortcut([...modifiers, event.code].join('+'))
  }

  async function downloadUpdate() {
    if (operation.current) return
    operation.current = true
    setPhase('downloading')
    setProgress({ downloaded: 0, total: undefined })
    setMessage('正在下载更新…')
    try {
      await invoke('download_app_update')
      setPhase('ready')
      setMessage('更新已下载，可以安装')
    } catch (e) {
      setPhase('available')
      setMessage(String(e))
    } finally {
      operation.current = false
    }
  }

  async function installUpdate() {
    if (operation.current) return
    operation.current = true
    setPhase('installing')
    setMessage('正在安装更新…')
    try {
      await invoke('install_app_update')
    } catch (e) {
      setPhase('ready')
      setMessage(String(e))
      operation.current = false
    }
  }

  if (!isTauriRuntime()) return null
  const updating = ['checking', 'downloading', 'installing'].includes(phase)
  return (
    <div className="desktop-settings">
      <Button
        className="button full-width"
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={showMore}
        aria-controls="desktop-more-settings"
        onClick={(event) => {
          event.currentTarget.focus()
          setMoreOpen(true)
        }}
      >
        更多设置
      </Button>
      <Dialog
        id="desktop-more-settings"
        className="desktop-more-settings-dialog"
        open={showMore}
        onOpenChange={setMoreOpen}
        title="更多设置"
        closeLabel="关闭更多设置"
        footer={
          <Button disabled={phase === 'installing'} onClick={() => setMoreOpen(false)}>
            完成
          </Button>
        }
      >
        <div className="desktop-settings-content">
          <div className="desktop-field">
            <div className="desktop-update-row">
              <span className="desktop-setting-title">软件更新</span>
              <Button
                disabled={!preferences?.updaterConfigured || phase === 'checking' || phase === 'installing'}
                onClick={(event) => {
                  event.currentTarget.focus()
                  if (update) setShowUpdate(true)
                  else void checkUpdate()
                }}
              >
                {phase === 'checking' ? '检查中…' : update ? `新版本 ${update.version}` : '检查更新'}
              </Button>
            </div>
            <p className="desktop-update-message" role="status">
              {message || (preferences && !preferences.updaterConfigured ? '此版本暂未提供在线更新' : '')}
            </p>
          </div>
          <CheckBox
            className="desktop-autostart"
            checked={preferences?.autostart ?? false}
            disabled={!preferences || busy}
            onChange={(e) => void setAutostart(e.currentTarget.checked)}
          >
            开机自启
          </CheckBox>
          <div className="desktop-field">
            <span className="desktop-setting-title">键位绑定</span>
            <label className="desktop-label" htmlFor="clipboard-shortcut">
              发送当前剪贴板
            </label>
            <div className="desktop-shortcut-row">
              <TextBox
                id="clipboard-shortcut"
                ref={input}
                readOnly
                value={recording ? '请按快捷键…' : (preferences?.clipboardShortcut ?? '')}
                placeholder="未设置快捷键"
                onKeyDown={recordKey}
                onBlur={() => {
                  if (recording) void stopRecording()
                }}
              />
              <Button
                disabled={!preferences || busy}
                onClick={() => void (recording ? stopRecording() : startRecording())}
              >
                {recording ? '取消' : '录制'}
              </Button>
              <Button
                variant="ghost"
                disabled={!preferences?.clipboardShortcut || busy}
                onClick={() => void saveShortcut('')}
              >
                清除
              </Button>
            </div>
            {preferences?.shortcutError && (
              <span className="desktop-error" role="alert">
                {preferences.shortcutError}
              </span>
            )}
          </div>
          {error && (
            <div className="desktop-error" role="alert">
              {error}
              <Button variant="ghost" onClick={() => void refresh()}>
                重试
              </Button>
            </div>
          )}
        </div>
        <Dialog
          open={showUpdate}
          onOpenChange={(open) => {
            if (phase !== 'installing') setShowUpdate(open)
          }}
          title={`更新到 ${update?.version ?? ''}`}
          closeLabel="关闭更新详情"
          footer={
            <>
              <Button variant="ghost" disabled={updating} onClick={() => void checkUpdate()}>
                重新检查
              </Button>
              {phase === 'ready' || phase === 'installing' ? (
                <Button
                  variant="primary"
                  disabled={hasTransfers || phase === 'installing'}
                  onClick={() => void installUpdate()}
                >
                  安装并重启
                </Button>
              ) : (
                <Button variant="primary" disabled={updating} onClick={() => void downloadUpdate()}>
                  {phase === 'downloading' ? '下载中…' : '下载更新'}
                </Button>
              )}
            </>
          }
        >
          <div className="desktop-update-notes">{update?.notes || '此版本未提供更新说明。'}</div>
          {phase === 'downloading' && (
            <>
              <ProgressBar
                value={progress.total ? (progress.downloaded / progress.total) * 100 : undefined}
                aria-label="更新下载进度"
              />
              <span className="desktop-label">
                {formatBytes(progress.downloaded)}
                {progress.total ? ` / ${formatBytes(progress.total)}` : ''}
              </span>
            </>
          )}
          <span className="desktop-label" role="status">
            {message}
          </span>
          {hasTransfers && (
            <span className="desktop-label">请先完成或取消待处理、进行中及已暂停的传输，再安装更新。</span>
          )}
        </Dialog>
      </Dialog>
    </div>
  )
}

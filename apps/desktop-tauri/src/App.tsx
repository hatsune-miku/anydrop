import type { ReactNode } from 'react'
import { useEffect, useMemo, useRef, useState } from 'react'

import {
  Clipboard,
  Eye,
  FolderOpen,
  Minus,
  Pause,
  Pencil,
  Play,
  RefreshCw,
  Share2,
  Square,
  Upload,
  X,
} from 'lucide-react'

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'

import {
  emptySnapshot,
  fallbackSettings,
  formatBytes,
  formatSpeed,
  isTauriRuntime,
  percent,
  previewKind,
  transferStatus,
  type SettingsModel,
  type Snapshot,
  type Transfer,
} from './types'

// Detect macOS once at module level. On macOS we rely on the native window
// chrome (traffic lights + drag) and skip the custom titlebar entirely.
const isMac =
  typeof navigator !== 'undefined' &&
  (navigator.platform.startsWith('Mac') || navigator.userAgent.includes('Macintosh'))

function Surface({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <div className={`surface-shell ${className}`}>{children}</div>
}

/** Cross-platform basename for display (handles both `/` and `\` separators). */
function baseName(path: string): string {
  return (
    path
      .split(/[\\/]/)
      .filter(Boolean)
      .pop() ?? path
  )
}

/** A pending send awaiting user confirmation in the send-confirm dialog. */
type PendingSend = {
  hosts: string[]
  peerLabel: string
  paths: string[]
}

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot)
  const [selectedPeerName, setSelectedPeerName] = useState('')
  const [settingsDraft, setSettingsDraft] = useState<SettingsModel>(fallbackSettings)
  const [busy, setBusy] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [notice, setNotice] = useState('')
  const [pendingSend, setPendingSend] = useState<PendingSend | null>(null)
  const [editingRemark, setEditingRemark] = useState<string | null>(null)
  const [remarkDraft, setRemarkDraft] = useState('')
  const logListRef = useRef<HTMLDivElement>(null)

  const systemDark =
    typeof window !== 'undefined' ? (window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false) : false
  // Theme preference is stored natively in settings now (settings.darkMode):
  // null = follow the OS theme, an explicit boolean = user override.
  const darkMode = snapshot.settings.darkMode ?? systemDark

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode)
  }, [darkMode])

  // Auto-dismiss the toast notice after 5s so transient messages (e.g.
  // "收到新的剪贴板文本") don't linger.
  useEffect(() => {
    if (!notice) return
    const timer = setTimeout(() => setNotice(''), 5000)
    return () => clearTimeout(timer)
  }, [notice])

  // Auto-scroll the log panel to the newest entry whenever logs change.
  useEffect(() => {
    if (logListRef.current) {
      logListRef.current.scrollTop = logListRef.current.scrollHeight
    }
  }, [snapshot.logs])

  useEffect(() => {
    const suppressContextMenu = (event: MouseEvent) => event.preventDefault()
    window.addEventListener('contextmenu', suppressContextMenu)

    if (!isTauriRuntime()) {
      setNotice('浏览器预览模式：Tauri 服务接口未连接')
      return () => window.removeEventListener('contextmenu', suppressContextMenu)
    }

    void refresh()
    const unlisteners = [
      listen<Snapshot>('snapshot', (event) => {
        // Update the canonical snapshot only. Do NOT touch settingsDraft —
        // the backend's clipboard / discovery threads emit snapshots
        // frequently, and overwriting the draft here clobbers any unsaved
        // edits (notoriously: flipping the "double-copy" toggle, then having
        // it snap back the moment a clipboard event fires).
        setSnapshot(event.payload)
      }),
      listen<Transfer>('incoming-file', (event) => {
        setNotice(`收到文件请求：${event.payload.fileName}`)
      }),
      listen('text-received', () => {
        setNotice('收到新的剪贴板文本')
      }),
      listen<Transfer>('transfer-updated', (event) => {
        // Backend stopped emitting a full snapshot on every chunk-progress
        // event (too much IPC bandwidth in concurrent folder transfers), so
        // we have to fold the per-row update into snapshot.transfers
        // ourselves to keep progress bars live.
        const updated = event.payload
        setSnapshot((prev) => {
          const idx = prev.transfers.findIndex((t) => t.key === updated.key)
          let nextTransfers: Transfer[]
          if (idx >= 0) {
            nextTransfers = prev.transfers.slice()
            // Defensive merge: protect "set once" fields (localPath,
            // fileName) from being clobbered if the backend ever sends an
            // empty value mid-transfer. Observed on large transfers where
            // the "open folder" affordance would disappear after completion.
            const prevRow = nextTransfers[idx]
            nextTransfers[idx] = {
              ...updated,
              localPath: updated.localPath || prevRow.localPath,
              fileName: updated.fileName || prevRow.fileName,
            }
          } else {
            nextTransfers = [...prev.transfers, updated].sort((a, b) =>
              a.key.localeCompare(b.key)
            )
          }
          return { ...prev, transfers: nextTransfers }
        })
        if (updated.status === 7) {
          setNotice(`${updated.fileName} 传输完成`)
        }
      }),
    ]

    return () => {
      window.removeEventListener('contextmenu', suppressContextMenu)
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()))
    }
  }, [])

  useEffect(() => {
    // Peer-selection reconciliation only — settings draft is intentionally
    // left alone here (see snapshot listener above for rationale).
    if (!selectedPeerName && snapshot.peers.length > 0) {
      setSelectedPeerName(snapshot.peers[0].name)
    }
    if (selectedPeerName && !snapshot.peers.some((peer) => peer.name === selectedPeerName)) {
      setSelectedPeerName(snapshot.peers[0]?.name ?? '')
    }
  }, [selectedPeerName, snapshot])

  const selectedPeer = useMemo(
    () => snapshot.peers.find((peer) => peer.name === selectedPeerName),
    [selectedPeerName, snapshot.peers]
  )

  async function runCommand<T>(command: string, args?: Record<string, unknown>) {
    if (!isTauriRuntime()) {
      setNotice('需要在 Tauri 桌面窗口中执行此操作')
      return undefined as T
    }

    setBusy(true)
    setNotice('')
    try {
      const next = await invoke<T>(command, args)
      if (next && typeof next === 'object' && 'running' in next) {
        const snap = next as unknown as Snapshot
        setSnapshot(snap)
        setSettingsDraft(snap.settings)
      }
      return next
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error))
      throw error
    } finally {
      setBusy(false)
    }
  }

  async function refresh() {
    if (!isTauriRuntime()) {
      setNotice('浏览器预览模式：Tauri 服务接口未连接')
      return
    }

    // Keep the spinner up for at least one full 200ms turn so the animation
    // reads as a deliberate, lively refresh even when the round-trip is instant.
    setRefreshing(true)
    const started = Date.now()
    try {
      // Trigger a fresh discovery broadcast; ignore errors if service is offline.
      try {
        await invoke<Snapshot>('refresh_peers')
      } catch {}
      const next = await invoke<Snapshot>('get_snapshot')
      setSnapshot(next)
      setSettingsDraft(next.settings)
    } finally {
      const wait = Math.max(0, 200 - (Date.now() - started))
      setTimeout(() => setRefreshing(false), wait)
    }
  }

  async function toggleService() {
    await runCommand<Snapshot>(snapshot.running ? 'stop_service' : 'start_service')
  }

  async function saveSettings() {
    await runCommand<Snapshot>('save_settings', { settings: settingsDraft })
    setNotice('设置已保存')
  }

  /**
   * Flip a boolean setting and persist it immediately.
   *
   * Booleans behave like switches — users expect them to take effect on
   * click, not after a separate "save" step. We commit straight against
   * `snapshot.settings` (the last-saved values) so that any in-progress
   * draft edits (ports, group id, display name) are not accidentally
   * persisted alongside.
   */
  async function setBoolSetting(
    key:
      | 'sendClipboardEnabled'
      | 'receiveClipboardEnabled'
      | 'sendOnlyOnDoubleCopy'
      | 'syncImageEnabled'
      | 'clipboardPopupEnabled'
      | 'darkMode',
    value: boolean
  ) {
    const next = { ...snapshot.settings, [key]: value }
    setSettingsDraft((draft) => ({ ...draft, [key]: value }))
    await runCommand<Snapshot>('save_settings', { settings: next })
  }

  // Pick file(s) or a folder, then stage them in the send-confirm dialog rather
  // than firing off immediately — guards against accidental sends.
  async function stageSend(directory: boolean) {
    if (!selectedPeer) {
      return
    }
    const selected = await open({ multiple: !directory, directory })
    const paths = Array.isArray(selected) ? selected : selected ? [selected] : []
    if (paths.length === 0) {
      return
    }
    setPendingSend({
      hosts: selectedPeer.hosts,
      peerLabel: selectedPeer.remark ?? selectedPeer.label,
      paths,
    })
  }

  async function confirmSend() {
    if (!pendingSend) {
      return
    }
    const { hosts, paths } = pendingSend
    setPendingSend(null)
    await runCommand<Snapshot>('send_paths', { hosts, paths })
  }

  async function setDefaultSaveDir() {
    const picked = await open({ multiple: false, directory: true })
    if (typeof picked === 'string') {
      const next = { ...snapshot.settings, defaultSaveDir: picked }
      setSettingsDraft((draft) => ({ ...draft, defaultSaveDir: picked }))
      await runCommand<Snapshot>('save_settings', { settings: next })
    }
  }

  function beginEditRemark(peerName: string, current: string) {
    setEditingRemark(peerName)
    setRemarkDraft(current)
  }

  async function commitRemark(peerName: string) {
    setEditingRemark(null)
    await runCommand<Snapshot>('set_peer_remark', { hostname: peerName, remark: remarkDraft.trim() })
  }

  async function runWindowAction(event: React.MouseEvent, action: 'close' | 'minimize' | 'toggleMaximize') {
    event.stopPropagation()
    if (!isTauriRuntime()) {
      setNotice('窗口控制需要在 Tauri 桌面窗口中使用')
      return
    }

    const appWindow = getCurrentWindow()
    if (action === 'minimize') {
      await appWindow.minimize()
      return
    }
    if (action === 'toggleMaximize') {
      await appWindow.toggleMaximize()
      return
    }
    await appWindow.close()
  }

  // Pending incoming offers are handled by the native receive popup now, so the
  // main window only lists transfers that have moved past the decision stage.
  const transfers = snapshot.transfers.filter((transfer) => transfer.status !== 1)
  const selectedPeerHosts = selectedPeer?.hosts.join(' / ') ?? '等待设备发现'

  return (
    <main className="app-shell">
      {isMac ? (
        // macOS: fullSizeContentView via titleBarStyle=Overlay.  Native traffic
        // lights float over the top-left of the webview; we just need a thin
        // drag region that clears them. No title text, no border — keep it
        // visually invisible so the webview content meets the window edge.
        <header
          className="window-titlebar window-titlebar--mac"
          data-tauri-drag-region
          onDoubleClick={(e) => void runWindowAction(e, 'toggleMaximize')}
        />
      ) : (
        <header
          className="window-titlebar"
          data-tauri-drag-region
          onDoubleClick={(e) => void runWindowAction(e, 'toggleMaximize')}
        >
          <div className="window-title" data-tauri-drag-region>
            AnyDrop
          </div>
          <div className="window-controls">
            <button
              aria-label="最小化"
              className="window-control"
              type="button"
              onClick={(e) => void runWindowAction(e, 'minimize')}
            >
              <Minus size={13} strokeWidth={1.5} />
            </button>
            <button
              aria-label="最大化或还原"
              className="window-control"
              type="button"
              onClick={(e) => void runWindowAction(e, 'toggleMaximize')}
            >
              <Square size={11} strokeWidth={1.5} />
            </button>
            <button
              aria-label="关闭"
              className="window-control close"
              type="button"
              onClick={(e) => void runWindowAction(e, 'close')}
            >
              <X size={13} strokeWidth={1.5} />
            </button>
          </div>
        </header>
      )}

      {notice ? (
        <div className="notice" role="status">
          {notice}
        </div>
      ) : null}

      {pendingSend ? (
        <section className="dialog-backdrop" aria-live="polite">
          <div className="dialog">
            <div className="section-heading">
              <span>确认发送</span>
              <small>{pendingSend.paths.length} 个项目</small>
            </div>
            <div className="dialog-list">
              <article className="request-row">
                <div className="row-main">
                  <strong>发送到 {pendingSend.peerLabel}</strong>
                  <span>共 {pendingSend.paths.length} 个项目</span>
                </div>
              </article>
              {pendingSend.paths.slice(0, 8).map((path) => (
                <div className="confirm-path" key={path} title={path}>
                  {baseName(path)}
                </div>
              ))}
              {pendingSend.paths.length > 8 ? (
                <div className="confirm-path confirm-path--more">… 等 {pendingSend.paths.length} 个</div>
              ) : null}
            </div>
            <div className="card-footer">
              <button className="button" type="button" onClick={() => setPendingSend(null)}>
                取消
              </button>
              <button className="button primary" type="button" disabled={busy} onClick={() => void confirmSend()}>
                <Upload size={15} />
                确认发送
              </button>
            </div>
          </div>
        </section>
      ) : null}

      <section className="app-window">
        <header className="content-header">
          <div className="app-identity">
            <div>
              <h1>(˶'ᵕ'˶ {snapshot.running ? 'Daemon Alive!' : 'Daemon Stopped'}</h1>
              <p>{snapshot.statusText}</p>
            </div>
          </div>
          <div className="content-actions">
            <button className="button" type="button" disabled={busy} onClick={toggleService}>
              {snapshot.running ? <Square size={15} /> : <Play size={15} />}
              {snapshot.running ? '停止服务' : '启动服务'}
            </button>
          </div>
        </header>

        <div className="content-grid">
          <Surface className="device-pane">
            <section className="card full-height">
              <div className="section-heading">
                <span>Peers</span>
                <button
                  className="icon-button icon-button--ghost"
                  type="button"
                  aria-label="刷新"
                  title="刷新设备列表"
                  onClick={() => void refresh()}
                >
                  <RefreshCw size={14} className={refreshing ? 'spin' : undefined} />
                </button>
              </div>
              <div className="device-list">
                {snapshot.peers.length === 0 ? (
                  <p className="empty">暂无设备。开启服务后会自动发现同一网络中运行 AnyDrop 的设备。</p>
                ) : (
                  snapshot.peers.map((peer) => (
                    <div
                      className={peer.name === selectedPeerName ? 'device-row selected' : 'device-row'}
                      key={peer.name}
                      onClick={() => setSelectedPeerName(peer.name)}
                    >
                      <span className="device-dot" />
                      <span className="row-main">
                        {editingRemark === peer.name ? (
                          <input
                            className="remark-input"
                            autoFocus
                            value={remarkDraft}
                            placeholder={peer.name}
                            onClick={(e) => e.stopPropagation()}
                            onChange={(e) => setRemarkDraft(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') void commitRemark(peer.name)
                              if (e.key === 'Escape') setEditingRemark(null)
                            }}
                            onBlur={() => void commitRemark(peer.name)}
                          />
                        ) : (
                          <strong>{peer.remark ?? peer.label}</strong>
                        )}
                        <small>{peer.hosts.join(' / ')}</small>
                      </span>
                      <button
                        className="icon-button icon-button--ghost device-remark-edit"
                        type="button"
                        aria-label="备注"
                        title="设置本地备注名"
                        onClick={(e) => {
                          e.stopPropagation()
                          beginEditRemark(peer.name, peer.remark ?? '')
                        }}
                      >
                        <Pencil size={13} />
                      </button>
                    </div>
                  ))
                )}
              </div>
            </section>
          </Surface>

          <div className="main-stack main-stack--with-log">
            <Surface>
              <section className="card send-card">
                <div className="section-heading">
                  <span>发送</span>
                </div>
                <div className="target-row">
                  <div className="target-icon">
                    <Share2 size={18} />
                  </div>
                  <div className="row-main">
                    <strong>{selectedPeer?.label ?? '选择左侧设备'}</strong>
                    <span>{selectedPeerHosts}</span>
                  </div>
                </div>
                <div className="send-actions">
                  <button
                    className="button primary"
                    type="button"
                    disabled={!selectedPeer || busy}
                    onClick={() => void stageSend(false)}
                  >
                    <Upload size={16} />
                    选择文件
                  </button>
                  <button
                    className="button"
                    type="button"
                    disabled={!selectedPeer || busy}
                    onClick={() => void stageSend(true)}
                  >
                    <FolderOpen size={16} />
                    选择文件夹
                  </button>
                  <button
                    className="button"
                    type="button"
                    disabled={!snapshot.running || busy}
                    onClick={() => void runCommand<Snapshot>('send_clipboard_now')}
                  >
                    <Clipboard size={16} />
                    发送剪贴板
                  </button>
                </div>
              </section>
            </Surface>

            <Surface>
              <section className="card transfers-card">
                <div className="section-heading">
                  <span>传输</span>
                  <small>{transfers.length} 条记录</small>
                </div>
                {transfers.length === 0 ? (
                  <p className="empty">暂无传输记录。</p>
                ) : (
                  <div className="transfer-list">
                    {transfers.map((transfer) => (
                      <article
                        className={`transfer-row${transfer.error ? ' transfer-row--error' : ''}`}
                        key={transfer.key}
                      >
                        <div className="row-main">
                          <strong>{transfer.fileName}</strong>
                          <span>
                            {transfer.direction === 'incoming' ? '接收' : '发送'} · {transferStatus(transfer.status)} ·{' '}
                            {formatBytes(transfer.progress)} / {formatBytes(transfer.total)}
                            {transfer.status === 4 && transfer.speedBps > 0
                              ? ` · ${formatSpeed(transfer.speedBps)}`
                              : ''}
                          </span>
                          {transfer.error ? (
                            <span className="transfer-error" title={transfer.error}>
                              ⚠ {transfer.error}
                            </span>
                          ) : null}
                        </div>
                        <div className="progress-track">
                          <span style={{ width: `${percent(transfer)}%` }} />
                        </div>
                        <div className="row-actions">
                          {/* In-flight (status=4): Pause + Cancel.
                              Paused (status=9): Resume + Cancel.
                              Terminal (5/6/7/2): Open folder + Dismiss. */}
                          {transfer.status === 4 ? (
                            <>
                              <button
                                className="icon-button icon-button--ghost"
                                type="button"
                                aria-label="暂停"
                                title="暂停"
                                onClick={() =>
                                  void runCommand<Snapshot>('pause_transfer', {
                                    transferKey: transfer.key,
                                  })
                                }
                              >
                                <Pause size={15} />
                              </button>
                              <button
                                className="icon-button icon-button--ghost"
                                type="button"
                                aria-label="取消"
                                title="取消传输"
                                onClick={() =>
                                  void runCommand<Snapshot>('cancel_transfer', {
                                    transferKey: transfer.key,
                                  })
                                }
                              >
                                <X size={15} />
                              </button>
                            </>
                          ) : transfer.status === 9 ? (
                            <>
                              <button
                                className="icon-button icon-button--ghost"
                                type="button"
                                aria-label="继续"
                                title="继续传输"
                                onClick={() =>
                                  void runCommand<Snapshot>('resume_transfer', {
                                    transferKey: transfer.key,
                                  })
                                }
                              >
                                <Play size={15} />
                              </button>
                              <button
                                className="icon-button icon-button--ghost"
                                type="button"
                                aria-label="取消"
                                title="取消传输"
                                onClick={() =>
                                  void runCommand<Snapshot>('cancel_transfer', {
                                    transferKey: transfer.key,
                                  })
                                }
                              >
                                <X size={15} />
                              </button>
                            </>
                          ) : (
                            <>
                              {transfer.status === 7 && previewKind(transfer.fileName) !== 'other' ? (
                                <button
                                  className="icon-button"
                                  type="button"
                                  aria-label="速览"
                                  title="速览此文件"
                                  onClick={() =>
                                    void runCommand<void>('preview_file', {
                                      transferKey: transfer.key,
                                    })
                                  }
                                >
                                  <Eye size={15} />
                                </button>
                              ) : null}
                              {transfer.localPath ? (
                                <button
                                  className="icon-button"
                                  type="button"
                                  aria-label="打开所在目录"
                                  title="打开所在目录并选中"
                                  onClick={() =>
                                    void runCommand<void>('open_transfer_folder', {
                                      transferKey: transfer.key,
                                    })
                                  }
                                >
                                  <FolderOpen size={15} />
                                </button>
                              ) : null}
                              <button
                                className="icon-button"
                                type="button"
                                aria-label="从列表移除"
                                title="从列表移除"
                                onClick={() =>
                                  void runCommand<Snapshot>('dismiss_transfer', {
                                    transferKey: transfer.key,
                                  })
                                }
                              >
                                <X size={15} />
                              </button>
                            </>
                          )}
                        </div>
                      </article>
                    ))}
                  </div>
                )}
              </section>
            </Surface>
            <Surface>
              <section className="card log-card">
                <div className="section-heading">
                  <span>日志</span>
                  <button
                    className="button quiet"
                    type="button"
                    style={{ fontSize: 12, minHeight: 26, padding: '0 8px' }}
                    onClick={() => void runCommand<Snapshot>('clear_logs')}
                  >
                    清空
                  </button>
                </div>
                <div className="log-list" ref={logListRef}>
                  {snapshot.logs.length === 0 ? (
                    <p className="empty">暂无日志。</p>
                  ) : (
                    snapshot.logs.map((entry, i) => (
                      <div className="log-entry" key={i}>
                        {entry}
                      </div>
                    ))
                  )}
                </div>
              </section>
            </Surface>
          </div>

          {/* settings-pane--busy: dims and disables interaction during any
              in-flight backend call.  Saving a setting that requires a
              service restart (most of the toggles do) takes a couple of
              seconds — disable everything until it's done so the user
              doesn't queue up a second change while the first is mid-flight. */}
          <Surface className={`settings-pane${busy ? ' settings-pane--busy' : ''}`}>
            <section className="card full-height">
              <div className="section-heading">
                <span>设置</span>
                {busy ? <small>正在应用…</small> : null}
              </div>
              <label className="toggle-row">
                <input
                  checked={darkMode}
                  type="checkbox"
                  onChange={(event) => void setBoolSetting('darkMode', event.target.checked)}
                />
                <span>深色模式</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={snapshot.settings.sendClipboardEnabled}
                  type="checkbox"
                  onChange={(event) => void setBoolSetting('sendClipboardEnabled', event.target.checked)}
                />
                <span>发送本机剪贴板</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={snapshot.settings.receiveClipboardEnabled}
                  type="checkbox"
                  onChange={(event) => void setBoolSetting('receiveClipboardEnabled', event.target.checked)}
                />
                <span>接收远端剪贴板</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={snapshot.settings.sendOnlyOnDoubleCopy}
                  type="checkbox"
                  onChange={(event) => void setBoolSetting('sendOnlyOnDoubleCopy', event.target.checked)}
                />
                <span>仅双击复制时发送</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={snapshot.settings.syncImageEnabled}
                  type="checkbox"
                  onChange={(event) => void setBoolSetting('syncImageEnabled', event.target.checked)}
                />
                <span>同步剪贴板图片（最大 64MB）</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={snapshot.settings.clipboardPopupEnabled}
                  type="checkbox"
                  onChange={(event) => void setBoolSetting('clipboardPopupEnabled', event.target.checked)}
                />
                <span>收到剪贴板时弹窗提示</span>
              </label>
              <label className="field">
                <span>默认保存目录</span>
                <div className="field-row">
                  <input type="text" readOnly value={snapshot.settings.defaultSaveDir} title={snapshot.settings.defaultSaveDir} />
                  <button className="button" type="button" disabled={busy} onClick={() => void setDefaultSaveDir()}>
                    更改
                  </button>
                </div>
              </label>
              <label className="field">
                <span>本机外显名</span>
                <input
                  type="text"
                  placeholder={snapshot.settings.displayName || '（系统主机名）'}
                  value={settingsDraft.displayName}
                  onChange={(event) => setSettingsDraft({ ...settingsDraft, displayName: event.target.value })}
                />
              </label>
              <label className="field">
                <span>组 ID</span>
                <input
                  min={0}
                  type="number"
                  value={settingsDraft.groupIdentity}
                  onChange={(event) =>
                    setSettingsDraft({
                      ...settingsDraft,
                      groupIdentity: Number(event.target.value),
                    })
                  }
                />
              </label>
              {/* 端口写死成默认值并跨设备保持一致;暴露成可改字段会让用户
                  误以为可以一端改一端不改,实际上协议要求双方端口对齐才能
                  互相发现与传输。这里只读展示,保留可见性方便排障。 */}
              <div className="field-grid">
                <label className="field">
                  <span>发现端口</span>
                  <input type="number" value={snapshot.settings.discoveryPort} readOnly disabled />
                </label>
                <label className="field">
                  <span>传输端口</span>
                  <input type="number" value={snapshot.settings.dataPort} readOnly disabled />
                </label>
              </div>
              <button className="button full-width" type="button" disabled={busy} onClick={saveSettings}>
                保存设置
              </button>
            </section>
          </Surface>
        </div>
      </section>
    </main>
  )
}

export default App

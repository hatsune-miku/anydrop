import type { ReactNode } from 'react'
import { useEffect, useMemo, useRef, useState } from 'react'

import {
  Check,
  Clipboard,
  FolderOpen,
  Minus,
  Pause,
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

type SettingsModel = {
  sendClipboardEnabled: boolean
  receiveClipboardEnabled: boolean
  sendOnlyOnDoubleCopy: boolean
  groupIdentity: number
  discoveryPort: number
  dataPort: number
  displayName: string
}

type PeerGroup = {
  label: string
  name: string
  hosts: string[]
}

type Transfer = {
  key: string
  fileId: number
  fileName: string
  remotePath: string
  localPath: string
  peer: string
  host: string
  direction: 'incoming' | 'outgoing'
  progress: number
  total: number
  status: number
}

type Snapshot = {
  running: boolean
  settings: SettingsModel
  peers: PeerGroup[]
  transfers: Transfer[]
  lastClipboardText: string
  lastReceivedText: string
  statusText: string
  logs: string[]
}

// Detect macOS once at module level. On macOS we rely on the native window
// chrome (traffic lights + drag) and skip the custom titlebar entirely.
const isMac =
  typeof navigator !== 'undefined' &&
  (navigator.platform.startsWith('Mac') || navigator.userAgent.includes('Macintosh'))

const fallbackSettings: SettingsModel = {
  sendClipboardEnabled: true,
  receiveClipboardEnabled: true,
  sendOnlyOnDoubleCopy: false,
  groupIdentity: 0,
  discoveryPort: 9818,
  dataPort: 9819,
  displayName: '',
}

const emptySnapshot: Snapshot = {
  running: false,
  settings: fallbackSettings,
  peers: [],
  transfers: [],
  lastClipboardText: '',
  lastReceivedText: '',
  statusText: 'Loading',
  logs: [],
}

function Surface({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <div className={`surface-shell ${className}`}>{children}</div>
}

function transferStatus(status: number) {
  switch (status) {
    case 1:
      return '等待确认'
    case 2:
      return '已拒绝'
    case 3:
      return '已接受'
    case 4:
      return '传输中'
    case 5:
      return '发送方取消'
    case 6:
      return '接收方取消'
    case 7:
      return '完成'
    case 8:
      return '错误'
    default:
      return '未知'
  }
}

function formatBytes(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB']
  let size = value
  let index = 0
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024
    index += 1
  }
  return `${size.toFixed(index === 0 ? 0 : 1)} ${units[index]}`
}

function percent(transfer: Transfer) {
  if (transfer.total <= 0) {
    return 0
  }
  return Math.min(100, Math.round((transfer.progress / transfer.total) * 100))
}

function isTauriRuntime() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot)
  const [selectedPeerName, setSelectedPeerName] = useState('')
  const [settingsDraft, setSettingsDraft] = useState<SettingsModel>(fallbackSettings)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const logListRef = useRef<HTMLDivElement>(null)
  const [darkMode, setDarkMode] = useState(() => {
    try {
      const stored = localStorage.getItem('darkMode')
      if (stored !== null) return stored === 'true'
    } catch {}
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false
  })

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode)
    try { localStorage.setItem('darkMode', String(darkMode)) } catch {}
  }, [darkMode])

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
        setSnapshot(event.payload)
        setSettingsDraft(event.payload.settings)
      }),
      listen<Transfer>('incoming-file', (event) => {
        setNotice(`收到文件请求：${event.payload.fileName}`)
      }),
      listen('text-received', () => {
        setNotice('收到新的剪贴板文本')
      }),
      listen<Transfer>('transfer-updated', (event) => {
        if (event.payload.status === 7) {
          setNotice(`${event.payload.fileName} 传输完成`)
        }
      }),
    ]

    return () => {
      window.removeEventListener('contextmenu', suppressContextMenu)
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()))
    }
  }, [])

  useEffect(() => {
    setSettingsDraft(snapshot.settings)
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

    // Trigger a fresh discovery broadcast; ignore errors if service is offline.
    try {
      await invoke<Snapshot>('refresh_peers')
    } catch {}
    const next = await invoke<Snapshot>('get_snapshot')
    setSnapshot(next)
    setSettingsDraft(next.settings)
  }

  async function toggleService() {
    await runCommand<Snapshot>(snapshot.running ? 'stop_service' : 'start_service')
  }

  async function saveSettings() {
    await runCommand<Snapshot>('save_settings', { settings: settingsDraft })
    setNotice('设置已保存')
  }

  async function sendFiles() {
    if (!selectedPeer) {
      return
    }
    const selected = await open({
      multiple: true,
      directory: false,
    })
    const files = Array.isArray(selected) ? selected : selected ? [selected] : []
    if (files.length === 0) {
      return
    }
    await runCommand<Snapshot>('send_files_to_peer', {
      hosts: selectedPeer.hosts,
      files,
    })
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

  const incoming = snapshot.transfers.filter((transfer) => transfer.direction === 'incoming' && transfer.status === 1)
  const transfers = snapshot.transfers.filter((transfer) => transfer.status !== 1)
  const selectedPeerHosts = selectedPeer?.hosts.join(' / ') ?? '等待设备发现'

  return (
    <main className="app-shell">
      {!isMac && (
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

      {incoming.length > 0 ? (
        <section className="dialog-backdrop" aria-live="polite">
          <div className="dialog">
            <div className="section-heading">
              <span>文件接收</span>
              <small>{incoming.length} 个请求</small>
            </div>
            <div className="dialog-list">
              {incoming.map((transfer) => (
                <article className="request-row" key={transfer.key}>
                  <div className="row-main">
                    <strong>{transfer.fileName}</strong>
                    <span>
                      {transfer.peer} · {formatBytes(transfer.total)}
                    </span>
                  </div>
                  <div className="row-actions">
                    <button
                      className="button primary"
                      type="button"
                      onClick={() =>
                        void runCommand<Snapshot>('accept_transfer', {
                          transferKey: transfer.key,
                        })
                      }
                    >
                      <Check size={16} />
                      接收
                    </button>
                    <button
                      className="button"
                      type="button"
                      onClick={() =>
                        void runCommand<Snapshot>('reject_transfer', {
                          transferKey: transfer.key,
                        })
                      }
                    >
                      <X size={16} />
                      拒绝
                    </button>
                  </div>
                </article>
              ))}
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
            <button className="button quiet" type="button" onClick={() => void refresh()}>
              <RefreshCw size={15} />
              刷新
            </button>
            <button className="button primary" type="button" disabled={busy} onClick={toggleService}>
              {snapshot.running ? <Pause size={15} /> : <Play size={15} />}
              {snapshot.running ? '暂停' : '开启'}
            </button>
          </div>
        </header>

        <div className="content-grid">
          <Surface className="device-pane">
            <section className="card full-height">
              <div className="section-heading">
                <span>Peers</span>
              </div>
              <div className="device-list">
                {snapshot.peers.length === 0 ? (
                  <p className="empty">暂无设备。开启服务后会自动发现同一网络中运行 AnyDrop 的设备。</p>
                ) : (
                  snapshot.peers.map((peer) => (
                    <button
                      className={peer.name === selectedPeerName ? 'device-row selected' : 'device-row'}
                      key={peer.name}
                      type="button"
                      onClick={() => setSelectedPeerName(peer.name)}
                    >
                      <span className="device-dot" />
                      <span className="row-main">
                        <strong>{peer.label}</strong>
                        <small>{peer.hosts.join(' / ')}</small>
                      </span>
                    </button>
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
                    onClick={() => void sendFiles()}
                  >
                    <Upload size={16} />
                    选择文件
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
                      <article className="transfer-row" key={transfer.key}>
                        <div className="row-main">
                          <strong>{transfer.fileName}</strong>
                          <span>
                            {transfer.direction === 'incoming' ? '接收' : '发送'} · {transferStatus(transfer.status)} ·{' '}
                            {formatBytes(transfer.progress)} / {formatBytes(transfer.total)}
                          </span>
                        </div>
                        <div className="progress-track">
                          <span style={{ width: `${percent(transfer)}%` }} />
                        </div>
                        <div className="row-actions">
                          {transfer.localPath ? (
                            <button
                              className="icon-button"
                              type="button"
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
                            onClick={() =>
                              void runCommand<Snapshot>('dismiss_transfer', {
                                transferKey: transfer.key,
                              })
                            }
                          >
                            <X size={15} />
                          </button>
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
                      <div className="log-entry" key={i}>{entry}</div>
                    ))
                  )}
                </div>
              </section>
            </Surface>
          </div>

          <Surface className="settings-pane">
            <section className="card full-height">
              <div className="section-heading">
                <span>设置</span>
              </div>
              <label className="toggle-row">
                <input
                  checked={darkMode}
                  type="checkbox"
                  onChange={(event) => setDarkMode(event.target.checked)}
                />
                <span>深色模式</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={settingsDraft.sendClipboardEnabled}
                  type="checkbox"
                  onChange={(event) =>
                    setSettingsDraft({
                      ...settingsDraft,
                      sendClipboardEnabled: event.target.checked,
                    })
                  }
                />
                <span>同步本机剪贴板</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={settingsDraft.receiveClipboardEnabled}
                  type="checkbox"
                  onChange={(event) =>
                    setSettingsDraft({
                      ...settingsDraft,
                      receiveClipboardEnabled: event.target.checked,
                    })
                  }
                />
                <span>接收远端剪贴板</span>
              </label>
              <label className="toggle-row">
                <input
                  checked={settingsDraft.sendOnlyOnDoubleCopy}
                  type="checkbox"
                  onChange={(event) =>
                    setSettingsDraft({
                      ...settingsDraft,
                      sendOnlyOnDoubleCopy: event.target.checked,
                    })
                  }
                />
                <span>仅双击复制时发送</span>
              </label>
              <label className="field">
                <span>本机外显名</span>
                <input
                  type="text"
                  placeholder={snapshot.settings.displayName || '（系统主机名）'}
                  value={settingsDraft.displayName}
                  onChange={(event) =>
                    setSettingsDraft({ ...settingsDraft, displayName: event.target.value })
                  }
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
              <div className="field-grid">
                <label className="field">
                  <span>发现端口</span>
                  <input
                    min={1}
                    max={65535}
                    type="number"
                    value={settingsDraft.discoveryPort}
                    onChange={(event) =>
                      setSettingsDraft({
                        ...settingsDraft,
                        discoveryPort: Number(event.target.value),
                      })
                    }
                  />
                </label>
                <label className="field">
                  <span>传输端口</span>
                  <input
                    min={1}
                    max={65535}
                    type="number"
                    value={settingsDraft.dataPort}
                    onChange={(event) =>
                      setSettingsDraft({
                        ...settingsDraft,
                        dataPort: Number(event.target.value),
                      })
                    }
                  />
                </label>
              </div>
              <button className="button primary full-width" type="button" disabled={busy} onClick={saveSettings}>
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

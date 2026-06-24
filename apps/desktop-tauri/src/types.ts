// Shared data shapes mirrored from the Rust backend (serde camelCase), plus
// small formatting helpers. Used by the main window, the receive popup, and
// the preview window so the contract lives in one place.

export type SettingsModel = {
  sendClipboardEnabled: boolean
  receiveClipboardEnabled: boolean
  sendOnlyOnDoubleCopy: boolean
  groupIdentity: number
  discoveryPort: number
  dataPort: number
  displayName: string
  syncImageEnabled: boolean
  /** null = follow OS theme; true/false = explicit user choice. */
  darkMode: boolean | null
  /** Default directory received files land in. */
  defaultSaveDir: string
  /** Show a bottom-right popup when clipboard content arrives from a peer. */
  clipboardPopupEnabled: boolean
}

export type PeerGroup = {
  label: string
  name: string
  hosts: string[]
  /** Local-only nickname; never advertised to the peer. */
  remark?: string
}

export type Transfer = {
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
  /** Latest error reported for the transfer, if any. Sticky once set. */
  error?: string
  /** Smoothed transfer rate in bytes/sec; 0 at start/terminal. */
  speedBps: number
}

export type Snapshot = {
  running: boolean
  settings: SettingsModel
  peers: PeerGroup[]
  transfers: Transfer[]
  lastClipboardText: string
  lastReceivedText: string
  statusText: string
  logs: string[]
}

export const fallbackSettings: SettingsModel = {
  sendClipboardEnabled: true,
  receiveClipboardEnabled: true,
  sendOnlyOnDoubleCopy: false,
  groupIdentity: 0,
  discoveryPort: 9818,
  dataPort: 9819,
  displayName: '',
  syncImageEnabled: false,
  darkMode: null,
  defaultSaveDir: '',
  clipboardPopupEnabled: false,
}

export const emptySnapshot: Snapshot = {
  running: false,
  settings: fallbackSettings,
  peers: [],
  transfers: [],
  lastClipboardText: '',
  lastReceivedText: '',
  statusText: 'Loading',
  logs: [],
}

export function transferStatus(status: number): string {
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
      return '已取消'
    case 6:
      return '错误'
    case 7:
      return '完成'
    case 8:
      return '错误'
    case 9:
      return '已暂停'
    default:
      return '未知'
  }
}

export function formatBytes(value: number): string {
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

export function formatSpeed(bps: number): string {
  if (!Number.isFinite(bps) || bps <= 0) {
    return ''
  }
  return `${formatBytes(bps)}/s`
}

export function percent(transfer: Transfer): number {
  if (transfer.total <= 0) {
    return 0
  }
  return Math.min(100, Math.round((transfer.progress / transfer.total) * 100))
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * Classify a file name for preview. Mirrors the backend `preview_kind`. Only
 * image / audio / video are previewable; everything else (archives, docs, …)
 * is `other` and gets no preview affordance.
 */
export function previewKind(name: string): 'image' | 'audio' | 'video' | 'other' {
  const dot = name.lastIndexOf('.')
  const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : ''
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico', 'avif'].includes(ext)) return 'image'
  if (['mp3', 'wav', 'flac', 'aac', 'ogg', 'oga', 'm4a', 'opus'].includes(ext)) return 'audio'
  if (['mp4', 'webm', 'mov', 'm4v', 'ogv'].includes(ext)) return 'video'
  return 'other'
}

/** Resolve which window surface to render from the `?window=` query param. */
export function windowKind(): 'main' | 'receive' | 'preview' {
  if (typeof window === 'undefined') return 'main'
  const value = new URLSearchParams(window.location.search).get('window')
  if (value === 'receive' || value === 'preview') return value
  return 'main'
}

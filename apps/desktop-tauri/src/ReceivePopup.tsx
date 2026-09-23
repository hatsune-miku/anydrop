import { useEffect, useMemo, useRef, useState } from 'react'

import { Check, Copy, FolderInput, FolderOpen, TriangleAlert, X } from 'lucide-react'

import { Button, CakeProvider, ProgressBar } from '@a1knla/cakeui'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'

import {
  type PeerGroup,
  type Snapshot,
  type Transfer,
  formatBytes,
  formatSpeed,
  percent,
  transferStatus,
} from './types'

type ClipboardCard = {
  id: number
  kind: 'text' | 'image'
  preview: string
  /** Full text (text cards only), used by the copy button. */
  text: string
  peer: string
  title?: string
}

type OutgoingRequest = { id: string; clipboard: boolean; fileName: string; bytes: number }

/** Statuses we still consider "live" for the popup's tracking heuristic. */
const ACTIVE = new Set([1, 4, 9])

function isTerminal(status: number) {
  return !ACTIVE.has(status)
}

export default function ReceivePopup() {
  const [darkMode, setDarkMode] = useState(window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false)
  const [transfers, setTransfers] = useState<Record<string, Transfer>>({})
  // Keys the popup is responsible for showing. Seeded from incoming-file
  // events and any already-active incoming transfers in the initial snapshot.
  const [tracked, setTracked] = useState<Set<string>>(new Set())
  const [overrides, setOverrides] = useState<Record<string, string>>({})
  const [cards, setCards] = useState<ClipboardCard[]>([])
  const [requests, setRequests] = useState<OutgoingRequest[]>([])
  const [peers, setPeers] = useState<PeerGroup[]>([])
  const [selected, setSelected] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState<string | null>(null)
  const [requestError, setRequestError] = useState<Record<string, string>>({})
  const content = useRef<HTMLDivElement>(null)
  const cardSeq = useRef(0)
  // Last card signature + timestamp, to drop duplicate emits (network dup or a
  // dev StrictMode double-subscribe) that arrive back-to-back.
  const lastCard = useRef<{ sig: string; ts: number }>({ sig: '', ts: 0 })

  const track = (key: string) =>
    setTracked((prev) => {
      if (prev.has(key)) return prev
      const next = new Set(prev)
      next.add(key)
      return next
    })

  useEffect(() => {
    // The popup window is transparent — only the bubbles are visible. Clear any
    // page background this webview inherited from :root.
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'
    // Seed from current backend state so an offer that landed before this
    // window finished mounting is still shown.
    const systemDark = window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false
    void invoke<Snapshot>('get_snapshot')
      .then((snap) => {
        setDarkMode(snap.settings.darkMode ?? systemDark)
        setPeers(snap.peers)
        const map: Record<string, Transfer> = {}
        for (const t of snap.transfers) map[t.key] = t
        setTransfers(map)
        setTracked(
          new Set(snap.transfers.filter((t) => t.direction === 'incoming' && ACTIVE.has(t.status)).map((t) => t.key))
        )
      })
      .catch(() => {})

    let requestsChanged = false
    const requestsListener = listen<OutgoingRequest[]>('outgoing-requests', ({ payload }) => {
      requestsChanged = true
      setRequests(payload)
    })
    // Subscribe first, then read pending state: startup never drops a file-menu request.
    void requestsListener
      .then(() => invoke<OutgoingRequest[]>('get_outgoing_requests'))
      .then((initial) => {
        if (!requestsChanged) setRequests(initial)
      })
      .catch(() => {})
    const unlisteners = [
      requestsListener,
      listen<Transfer>('incoming-file', (event) => {
        const t = event.payload
        setTransfers((prev) => ({ ...prev, [t.key]: t }))
        track(t.key)
      }),
      listen<Transfer>('transfer-updated', (event) => {
        const t = event.payload
        setTransfers((prev) => {
          const prevRow = prev[t.key]
          return {
            ...prev,
            [t.key]: prevRow
              ? { ...t, localPath: t.localPath || prevRow.localPath, fileName: t.fileName || prevRow.fileName }
              : t,
          }
        })
      }),
      listen<Snapshot>('snapshot', (event) => {
        setDarkMode(event.payload.settings.darkMode ?? systemDark)
        setPeers(event.payload.peers)
        const map: Record<string, Transfer> = {}
        for (const t of event.payload.transfers) map[t.key] = t
        setTransfers(map)
      }),
      listen<{ kind: 'text' | 'image'; preview: string; text?: string; peer: string; title?: string }>(
        'clipboard-popup',
        (event) => {
          const {
            kind,
            preview,
            text = '',
            peer,
            title,
          } = event.payload ?? { kind: 'text', preview: '', text: '', peer: '' }
          // Skip empty text receipts — nothing useful to show, and an empty card
          // reads as a glitch.
          if (kind === 'text' && !preview.trim() && !peer.trim()) return
          // Drop a back-to-back duplicate of the same content.
          const sig = `${kind}:${preview}:${peer}`
          const now = Date.now()
          if (lastCard.current.sig === sig && now - lastCard.current.ts < 1500) return
          lastCard.current = { sig, ts: now }
          const id = (cardSeq.current += 1)
          setCards((prev) => [{ id, kind, preview, text, peer, title }, ...prev].slice(0, 4))
          // Auto-dismiss clipboard cards after a few seconds.
          setTimeout(() => setCards((prev) => prev.filter((c) => c.id !== id)), 6000)
        }
      ),
    ]
    return () => {
      void Promise.all(unlisteners).then((items) => items.forEach((u) => u()))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const rows = useMemo(
    () =>
      Array.from(tracked)
        .map((key) => transfers[key])
        .filter((t): t is Transfer => Boolean(t))
        .sort((a, b) => a.key.localeCompare(b.key)),
    [tracked, transfers]
  )

  const pending = rows.filter((t) => t.status === 1)
  const ongoing = rows.filter((t) => t.status !== 1)

  // Self-hide when there is nothing left to show. Rust re-shows the window on
  // the next offer / clipboard receipt.
  useEffect(() => {
    if (rows.length === 0 && cards.length === 0 && requests.length === 0) {
      const timer = setTimeout(() => void getCurrentWindow().hide(), 400)
      return () => clearTimeout(timer)
    }
    // Restore delayed send confirmations/receipts after first hydration. Incoming
    // file rows alone must still respect the backend's fullscreen-game suppression.
    if (requests.length > 0 || cards.length > 0) {
      void getCurrentWindow()
        .show()
        .catch(() => {})
    }
  }, [rows.length, cards.length, requests.length])

  useEffect(() => {
    if (!content.current) return
    const element = content.current
    let frame = 0
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        void invoke('resize_receive_window', { height: Math.ceil(element.getBoundingClientRect().height) + 192 }).catch(
          () => {}
        )
      })
    })
    observer.observe(element)
    return () => {
      observer.disconnect()
      cancelAnimationFrame(frame)
    }
  }, [])

  async function confirmRequest(request: OutgoingRequest, peerName: string) {
    if (busy || !peerName) return
    setBusy(request.id)
    setRequestError((prev) => ({ ...prev, [request.id]: '' }))
    try {
      await invoke('confirm_outgoing_request', { id: request.id, peerName })
    } catch (error) {
      setRequestError((prev) => ({ ...prev, [request.id]: String(error) }))
    } finally {
      setBusy(null)
    }
  }

  async function chooseLocation(key: string) {
    const picked = await open({ multiple: false, directory: true })
    if (typeof picked === 'string') {
      setOverrides((prev) => ({ ...prev, [key]: picked }))
    }
  }

  async function accept(key: string) {
    track(key)
    await invoke('accept_transfer', { transferKey: key, saveDir: overrides[key] ?? null })
  }

  // "已读" only closes the item in the popup — the transfer record stays in the
  // main "最近传输" list. (It does NOT delete the record.)
  function dismiss(key: string) {
    setTracked((prev) => {
      const next = new Set(prev)
      next.delete(key)
      return next
    })
  }

  // Clipboard receipts are throwaway — clicking anywhere on the card removes
  // just that one, leaving any other cards / transfers untouched.
  function dismissCard(id: number) {
    setCards((prev) => prev.filter((c) => c.id !== id))
  }

  return (
    <CakeProvider className="app-theme popup-shell" theme="pink" density="compact" mode={darkMode ? 'dark' : 'light'}>
      <div className="popup-body">
        <div className="popup-content" ref={content}>
          {requests.map((request) => {
            const choice = peers.length === 1 ? peers[0].name : (selected[request.id] ?? '')
            const target = peers.find((peer) => peer.name === choice)
            return (
              <article className="popup-card popup-card--offer" key={request.id}>
                <div className="row-main">
                  <strong>{request.clipboard ? '确认以文件形式发送' : '使用 AnyDrop 发送'}</strong>
                  <span>
                    {request.fileName} · {formatBytes(request.bytes)}
                  </span>
                </div>
                {peers.length > 1 ? (
                  <div className="popup-peers" role="group" aria-label="选择接收设备">
                    {peers.map((peer) => (
                      <Button
                        key={peer.name}
                        aria-pressed={choice === peer.name}
                        disabled={busy !== null}
                        variant={choice === peer.name ? 'primary' : 'default'}
                        onClick={() => setSelected((prev) => ({ ...prev, [request.id]: peer.name }))}
                      >
                        {peer.remark || peer.label}
                      </Button>
                    ))}
                  </div>
                ) : (
                  <span className="popup-dest">
                    {target ? `发送到：${target.remark || target.label}` : '当前频段暂无在线设备'}
                  </span>
                )}
                {requestError[request.id] && (
                  <span className="transfer-error" role="alert">
                    {requestError[request.id]}
                  </span>
                )}
                <div className="popup-actions">
                  <Button
                    variant="primary"
                    disabled={!target || busy !== null}
                    onClick={() => void confirmRequest(request, target!.name)}
                  >
                    {busy === request.id ? '发送中…' : '确认发送'}
                  </Button>
                  <Button
                    disabled={busy === request.id}
                    onClick={() => void invoke('dismiss_outgoing_request', { id: request.id })}
                  >
                    取消
                  </Button>
                </div>
              </article>
            )
          })}
          {cards.map((card, idx) => (
            <article
              className="popup-card popup-card--clip"
              key={`card-${card.id}`}
              role="button"
              tabIndex={0}
              title="点击关闭"
              onClick={() => dismissCard(card.id)}
            >
              <div className="row-main">
                <strong>{card.title || (card.kind === 'image' ? '收到剪贴板图片' : '收到剪贴板文本')}</strong>
                <span>{card.preview || card.peer}</span>
              </div>
              {card.kind === 'text' ? (
                <div className="popup-card-actions">
                  {idx === 0 ? <small className="copied-tag">已复制</small> : null}
                  <Button
                    variant="ghost"
                    className="icon-button"
                    type="button"
                    aria-label="复制"
                    title="复制到剪贴板"
                    onClick={(e) => {
                      // Copying is a distinct action — don't let it also dismiss the card.
                      e.stopPropagation()
                      void invoke('copy_text', { text: card.text })
                    }}
                  >
                    <Copy size={15} />
                  </Button>
                </div>
              ) : null}
            </article>
          ))}

          {pending.map((t) => (
            <article className="popup-card popup-card--offer" key={t.key}>
              <div className="row-main">
                <strong>{t.fileName}</strong>
                <span>
                  {t.peer} · {formatBytes(t.total)}
                </span>
                <small className="popup-dest" title={overrides[t.key]}>
                  保存到：{overrides[t.key] ?? '默认目录'}
                </small>
              </div>
              <div className="popup-actions">
                <Button variant="primary" className="button" type="button" onClick={() => void accept(t.key)}>
                  <Check size={15} />
                  接收
                </Button>
                <Button className="button" type="button" onClick={() => void chooseLocation(t.key)}>
                  <FolderInput size={15} />
                  更改位置
                </Button>
                <Button
                  className="button"
                  type="button"
                  onClick={() => void invoke('reject_transfer', { transferKey: t.key })}
                >
                  <X size={15} />
                  拒绝
                </Button>
              </div>
            </article>
          ))}

          {ongoing.map((t) => (
            <article className={`popup-card${t.error ? ' popup-card--error' : ''}`} key={t.key}>
              <div className="row-main">
                <strong>{t.fileName}</strong>
                <span>
                  {transferStatus(t.status)} · {formatBytes(t.progress)} / {formatBytes(t.total)}
                  {t.status === 4 && t.speedBps > 0 ? ` · ${formatSpeed(t.speedBps)}` : ''}
                </span>
                {t.error ? (
                  <small className="transfer-error" title={t.error}>
                    <TriangleAlert size={12} />
                    {t.error}
                  </small>
                ) : null}
              </div>
              <ProgressBar className="progress-track" value={percent(t)} max={100} aria-label="传输进度" />
              {isTerminal(t.status) ? (
                <div className="popup-actions">
                  {t.localPath ? (
                    <Button
                      className="button"
                      type="button"
                      onClick={() => void invoke('open_transfer_folder', { transferKey: t.key })}
                    >
                      <FolderOpen size={15} />
                      打开目录
                    </Button>
                  ) : null}
                  <Button className="button" type="button" onClick={() => dismiss(t.key)}>
                    <Check size={15} />
                    已读
                  </Button>
                </div>
              ) : null}
            </article>
          ))}
        </div>
      </div>
    </CakeProvider>
  )
}

import { useEffect, useMemo, useRef, useState } from 'react'

import { Check, Copy, FolderInput, FolderOpen, TriangleAlert, X } from 'lucide-react'

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'

import {
  formatBytes,
  formatSpeed,
  percent,
  transferStatus,
  type Snapshot,
  type Transfer,
} from './types'

type ClipboardCard = {
  id: number
  kind: 'text' | 'image'
  preview: string
  /** Full text (text cards only), used by the copy button. */
  text: string
  peer: string
}

/** Statuses we still consider "live" for the popup's tracking heuristic. */
const ACTIVE = new Set([1, 4, 9])

function isTerminal(status: number) {
  return !ACTIVE.has(status)
}

export default function ReceivePopup() {
  const [transfers, setTransfers] = useState<Record<string, Transfer>>({})
  // Keys the popup is responsible for showing. Seeded from incoming-file
  // events and any already-active incoming transfers in the initial snapshot.
  const [tracked, setTracked] = useState<Set<string>>(new Set())
  const [overrides, setOverrides] = useState<Record<string, string>>({})
  const [cards, setCards] = useState<ClipboardCard[]>([])
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
        document.documentElement.classList.toggle('dark', snap.settings.darkMode ?? systemDark)
        const map: Record<string, Transfer> = {}
        for (const t of snap.transfers) map[t.key] = t
        setTransfers(map)
        setTracked(
          new Set(
            snap.transfers
              .filter((t) => t.direction === 'incoming' && ACTIVE.has(t.status))
              .map((t) => t.key)
          )
        )
      })
      .catch(() => {})

    const unlisteners = [
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
        const map: Record<string, Transfer> = {}
        for (const t of event.payload.transfers) map[t.key] = t
        setTransfers(map)
      }),
      listen<{ kind: 'text' | 'image'; preview: string; text?: string; peer: string }>(
        'clipboard-popup',
        (event) => {
          const {
            kind,
            preview,
            text = '',
            peer,
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
          setCards((prev) => [{ id, kind, preview, text, peer }, ...prev].slice(0, 4))
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
    if (rows.length === 0 && cards.length === 0) {
      const timer = setTimeout(() => void getCurrentWindow().hide(), 400)
      return () => clearTimeout(timer)
    }
  }, [rows.length, cards.length])

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
    <div className="popup-shell">
      <div className="popup-body">
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
              <strong>{card.kind === 'image' ? '收到剪贴板图片' : '收到剪贴板文本'}</strong>
              <span>{card.preview || card.peer}</span>
            </div>
            {card.kind === 'text' ? (
              <div className="popup-card-actions">
                {idx === 0 ? <small className="copied-tag">已复制</small> : null}
                <button
                  className="icon-button icon-button--ghost"
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
                </button>
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
              <button className="button primary" type="button" onClick={() => void accept(t.key)}>
                <Check size={15} />
                接收
              </button>
              <button className="button" type="button" onClick={() => void chooseLocation(t.key)}>
                <FolderInput size={15} />
                更改位置
              </button>
              <button
                className="button"
                type="button"
                onClick={() => void invoke('reject_transfer', { transferKey: t.key })}
              >
                <X size={15} />
                拒绝
              </button>
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
            <div className="progress-track">
              <span style={{ width: `${percent(t)}%` }} />
            </div>
            {isTerminal(t.status) ? (
              <div className="popup-actions">
                {t.localPath ? (
                  <button
                    className="button"
                    type="button"
                    onClick={() => void invoke('open_transfer_folder', { transferKey: t.key })}
                  >
                    <FolderOpen size={15} />
                    打开目录
                  </button>
                ) : null}
                <button className="button" type="button" onClick={() => dismiss(t.key)}>
                  <Check size={15} />
                  已读
                </button>
              </div>
            ) : null}
          </article>
        ))}
      </div>
    </div>
  )
}

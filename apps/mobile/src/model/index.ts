export interface Peer {
  id: string
  name: string
  host: string
  port: number
}
export interface Snapshot {
  running: boolean
  group: number
  displayName: string
  mode: 'light' | 'dark' | 'system'
  receiveText: boolean
  peers: Peer[]
  lastReceived: string
  status: string
}
export function parseGroup(text: string): number {
  if (!/^\d+$/.test(text)) throw new Error('频段必须是 0 到 4294967295 之间的整数')
  const value = Number(text)
  if (!Number.isSafeInteger(value) || value > 4294967295) throw new Error('频段必须是 0 到 4294967295 之间的整数')
  return value
}

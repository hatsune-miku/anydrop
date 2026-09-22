import { useEffect, useState } from 'react'
import { AppState } from 'react-native'
import { SafeAreaProvider } from 'react-native-safe-area-context'
import { CakeProvider } from '@a1knla/cakeui-native'
import NativeAnyDrop from '../../specs/NativeAnyDrop'
import type { Snapshot } from '../../model'
import { Home } from './components/Home'

export function MobileApp() {
  const [snapshot, setSnapshot] = useState<Snapshot>()
  const [error, setError] = useState('')
  useEffect(() => {
    let active = true
    let reading = false
    async function refresh() {
      if (reading || AppState.currentState !== 'active') return
      reading = true
      try {
        const data = JSON.parse(await NativeAnyDrop.getSnapshot()) as Snapshot
        if (active) setSnapshot(data)
      } catch (failure) {
        if (active) setError(String(failure))
      } finally {
        reading = false
      }
    }
    void refresh()
    const timer = setInterval(() => {
      void refresh()
    }, 1500)
    const listener = AppState.addEventListener('change', () => {
      void refresh()
    })
    return () => {
      active = false
      clearInterval(timer)
      listener.remove()
    }
  }, [])
  return (
    <SafeAreaProvider>
      <CakeProvider mode={snapshot?.mode ?? 'system'}>
        <Home snapshot={snapshot} update={setSnapshot} error={error} setError={setError} />
      </CakeProvider>
    </SafeAreaProvider>
  )
}

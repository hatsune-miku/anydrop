import { useEffect, useMemo, useState } from 'react'

import { createThemeConfig, resolveThemeOptions, updateThemeConfig, validateThemeConfig } from '@a1knla/cakeui/theme'
import type { ThemeStorageAdapter } from '@a1knla/cakeui/theme'
import { invoke } from '@tauri-apps/api/core'

import type { AppearanceSettings, Snapshot } from '../types'

export function useAppearance(settings: AppearanceSettings) {
  const { appearanceConfig, appearanceTheme, darkMode: savedDarkMode } = settings
  const config = useMemo(() => {
    const saved = validateThemeConfig(appearanceConfig)
    // Existing fields remain the active scheme/mode controls for the main
    // window and local API. CakeUI retains all other scheme options.
    return updateThemeConfig(saved.valid ? saved.config : createThemeConfig(), {
      scheme: appearanceTheme ?? 'pink',
      mode: savedDarkMode == null ? 'system' : savedDarkMode ? 'dark' : 'light',
    })
  }, [appearanceConfig, appearanceTheme, savedDarkMode])
  // Auxiliary windows have no visible settings entry, but component preferences use the same native storage.
  const storage = useMemo<ThemeStorageAdapter>(
    () => ({
      load: () => null,
      save: async (next) => {
        const current = await invoke<Snapshot>('get_snapshot')
        const { mode } = resolveThemeOptions(next)
        await invoke('save_settings', {
          settings: {
            ...current.settings,
            appearanceConfig: next,
            appearanceTheme: next.scheme,
            darkMode: mode === 'system' ? null : mode === 'dark',
          },
        })
      },
    }),
    []
  )
  const [systemDark, setSystemDark] = useState(window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false)
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    function changed(event: MediaQueryListEvent) {
      setSystemDark(event.matches)
    }
    media.addEventListener('change', changed)
    return () => media.removeEventListener('change', changed)
  }, [])
  const darkMode = savedDarkMode ?? systemDark
  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode)
  }, [darkMode])
  return {
    darkMode,
    storage,
    provider: {
      locale: 'zh-CN' as const,
      defaultConfig: config,
      density: settings.appearanceDensity ?? 'compact',
    },
  }
}

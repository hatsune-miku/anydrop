import { useMemo, useRef, useState } from 'react'

import { Palette } from 'lucide-react'

import { ComboBox, ThemeSettings } from '@a1knla/cakeui'
import { resolveThemeOptions, themePresets } from '@a1knla/cakeui/theme'
import type { ThemeStorageAdapter } from '@a1knla/cakeui/theme'

import './index.scss'

import type { AppearanceSettings as AppearanceValues } from '../../../../types'

const themeLabels = {
  open: '主题设置',
  title: '主题设置',
  scheme: '主题色',
  custom: '自定义',
  color: 'HEX / RGB',
  colorPicker: '选择自定义颜色',
  mode: '明暗模式',
  system: '跟随系统',
  light: '浅色',
  dark: '深色',
  foreground: '文字颜色',
  auto: '自动',
  lightForeground: '浅色文字',
  darkForeground: '深色文字',
  apply: '应用',
  cancel: '取消',
  reset: '重置当前配色',
  invalidColor: '请输入不含透明度的 HEX 或 RGB 颜色',
  storageError: '主题未能保存，请重试',
  retry: '重试',
  loading: '正在读取主题…',
}

export default function AppearanceSettings({
  settings,
  disabled,
  onChange,
}: {
  settings: AppearanceValues
  disabled: boolean
  onChange: (change: Partial<AppearanceValues>) => Promise<void>
}) {
  // Let ThemeSettings use aria-disabled during its own save so focus can return.
  const [savingTheme, setSavingTheme] = useState(false)
  const changeRef = useRef(onChange)
  changeRef.current = onChange
  const storage = useMemo<ThemeStorageAdapter>(
    () => ({
      // The host snapshot already loads the native settings for all windows.
      load: () => null,
      save: async (config) => {
        const { mode } = resolveThemeOptions(config)
        setSavingTheme(true)
        try {
          await changeRef.current({
            appearanceConfig: config,
            appearanceTheme: config.scheme,
            darkMode: mode === 'system' ? null : mode === 'dark',
          })
        } finally {
          setSavingTheme(false)
        }
      },
    }),
    []
  )
  const schemeName =
    settings.appearanceTheme === 'custom'
      ? '自定义'
      : (themePresets.find((preset) => preset.id === (settings.appearanceTheme ?? 'pink'))?.name ?? '粉')

  return (
    <section className="desktop-appearance" aria-labelledby="desktop-appearance-title">
      <div className="desktop-setting-row">
        <Palette className="desktop-setting-icon" size={19} aria-hidden="true" />
        <h3 className="desktop-setting-title" id="desktop-appearance-title">
          外观
        </h3>
      </div>
      <div className="desktop-appearance-fields">
        <div className="desktop-appearance-field">
          <label className="desktop-label" htmlFor="appearance-theme">
            主题色与明暗
          </label>
          <ThemeSettings
            id="appearance-theme"
            disabled={disabled && !savingTheme}
            storage={storage}
            labels={themeLabels}
          >
            {schemeName}
          </ThemeSettings>
        </div>
        <div className="desktop-appearance-field">
          <label className="desktop-label" htmlFor="appearance-density">
            界面密度
          </label>
          <ComboBox
            id="appearance-density"
            value={settings.appearanceDensity ?? 'compact'}
            disabled={disabled}
            onChange={(event) =>
              void onChange({
                appearanceDensity: event.currentTarget.value as AppearanceValues['appearanceDensity'],
              }).catch(() => {})
            }
          >
            <option value="compact">紧凑</option>
            <option value="comfortable">舒适</option>
          </ComboBox>
        </div>
      </div>
    </section>
  )
}

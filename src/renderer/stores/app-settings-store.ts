import { create } from 'zustand'
import type { AppSettings, AppSettingsUpdate } from '@/types/settings'
import { DEFAULT_APP_SETTINGS } from '@/types/settings'

/**
 * What the boot read of settings/app found. `missing` is a first run
 * (KEY_NOT_FOUND); `failed` means the store holds defaults that are not the
 * user's settings, and the host refuses writes to the key until a later read
 * works.
 */
export type AppSettingsHostRead = 'pending' | 'found' | 'missing' | 'failed'

interface AppSettingsState {
  settings: AppSettings
  isLoaded: boolean
  hostRead: AppSettingsHostRead
  /** `hostRead` comes only from the boot read; a synced change from another device leaves it. */
  setSettings: (settings: AppSettings, hostRead?: Exclude<AppSettingsHostRead, 'pending'>) => void
  updateSetting: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void
  updateSettings: (updates: AppSettingsUpdate) => void
  resetToDefaults: () => void
}

export const useAppSettingsStore = create<AppSettingsState>((set) => ({
  settings: DEFAULT_APP_SETTINGS,
  isLoaded: false,
  hostRead: 'pending',

  setSettings: (settings, hostRead) =>
    set(hostRead ? { settings, isLoaded: true, hostRead } : { settings, isLoaded: true }),

  updateSetting: (key, value) =>
    set((state) => ({
      settings: { ...state.settings, [key]: value }
    })),

  updateSettings: (updates) =>
    set((state) => ({
      settings: { ...state.settings, ...updates }
    })),

  // Terminal profiles are the user's own launch entries, not preferences: a
  // reset keeps them, as it keeps MCP servers and custom agents. A reset is
  // not a first run, so it keeps the onboarding completion too.
  resetToDefaults: () =>
    set((state) => ({
      settings: {
        ...DEFAULT_APP_SETTINGS,
        terminalProfiles: state.settings.terminalProfiles,
        onboardingCompletedAt: state.settings.onboardingCompletedAt
      }
    }))
}))

// Selectors
export const useAppSettings = () => useAppSettingsStore((state) => state.settings)
export const useAppSettingsLoaded = () => useAppSettingsStore((state) => state.isLoaded)
export const useAppSettingsHostRead = () => useAppSettingsStore((state) => state.hostRead)
export const useOnboardingCompletedAt = () =>
  useAppSettingsStore((state) => state.settings.onboardingCompletedAt)
export const useTerminalFontFamily = () =>
  useAppSettingsStore((state) => state.settings.terminalFontFamily)
export const useTerminalFontSize = () =>
  useAppSettingsStore((state) => state.settings.terminalFontSize)
export const useDefaultShell = () => useAppSettingsStore((state) => state.settings.defaultShell)
export const useDefaultProjectColor = () =>
  useAppSettingsStore((state) => state.settings.defaultProjectColor)
export const useTerminalBufferSize = () =>
  useAppSettingsStore((state) => state.settings.terminalBufferSize)
export const useTerminalRenderer = () =>
  useAppSettingsStore((state) => state.settings.terminalRenderer)
// Strict `=== true` guards a corrupt or missing persisted value.
export const useTerminalScreenReaderMode = () =>
  useAppSettingsStore((state) => state.settings.terminalScreenReaderMode === true)
// Missing or corrupt persisted values stay on (the spec default).
export const useAutomaticCompletionRefresh = () =>
  useAppSettingsStore((state) => state.settings.automaticCompletionRefresh !== false)
export const useSuggestAutomations = () =>
  useAppSettingsStore((state) => state.settings.suggestAutomations !== false)
export const useConfirmTerminalClose = () =>
  useAppSettingsStore((state) => state.settings.confirmTerminalClose)
export const useLinkOpenMode = () => useAppSettingsStore((state) => state.settings.linkOpenMode)
export const useColorTheme = () => useAppSettingsStore((state) => state.settings.colorTheme)
export const useAppearanceMode = () => useAppSettingsStore((state) => state.settings.appearanceMode)
export const useUiZoomLevel = () => useAppSettingsStore((state) => state.settings.uiZoomLevel)
// Strict `=== true`: a missing or corrupt persisted value stays off.
export const useTranslucentRail = () =>
  useAppSettingsStore((state) => state.settings.translucentRail === true)
export const useAcpTurnTimeout = () =>
  useAppSettingsStore((state) => state.settings.acpTurnTimeoutSecs)
export const useEditorAutoSave = () => useAppSettingsStore((state) => state.settings.editorAutoSave)
export const useEditorAutoSaveDelayMs = () =>
  useAppSettingsStore((state) => state.settings.editorAutoSaveDelayMs)
export const useAcpTurnIdleTimeout = () =>
  useAppSettingsStore((state) => state.settings.acpTurnIdleTimeoutSecs)
export const useAcpSessionNewTimeout = () =>
  useAppSettingsStore((state) => state.settings.acpSessionNewTimeoutSecs)
export const useAcpSessionReopenTimeout = () =>
  useAppSettingsStore((state) => state.settings.acpSessionReopenTimeoutSecs)
export const useAgentIdleWindow = () =>
  useAppSettingsStore((state) => state.settings.agentIdleWindowSecs)
export const useNotifyOnTerminalIdle = () =>
  useAppSettingsStore((state) => state.settings.notifyOnTerminalIdle)
export const useNotifyOnAgentChatTurnFinished = () =>
  useAppSettingsStore((state) => state.settings.notifyOnAgentChatTurnFinished)
export const useNotifyOnAgentChatNeedsYou = () =>
  useAppSettingsStore((state) => state.settings.notifyOnAgentChatNeedsYou)

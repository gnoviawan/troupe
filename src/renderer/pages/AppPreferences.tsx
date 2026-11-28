import type { DetectedShells } from '@shared/types/ipc.types'
import { useEffect, useMemo, useState } from 'react'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { ComuSection } from '@/components/settings/ComuPolicySettings'
import { SettingsLayout } from '@/components/settings/SettingsLayout'
import { SettingsWindow } from '@/components/settings/SettingsWindow'
import {
  queueAgentIdleWindowPush,
  useResetAppSettings,
  useUpdateAppSetting
} from '@/hooks/use-app-settings'
import {
  useResetAllShortcuts,
  useResetShortcut,
  useUpdateShortcut
} from '@/hooks/use-keyboard-shortcuts'
import { acpApi, shellApi } from '@/lib/api'
import {
  useAcpSessionNewTimeout,
  useAcpSessionReopenTimeout,
  useAcpTurnIdleTimeout,
  useAcpTurnTimeout,
  useAgentIdleWindow
} from '@/stores/app-settings-store'
import { useKeyboardShortcutsStore } from '@/stores/keyboard-shortcuts-store'
import { useSettingsModalStore } from '@/stores/settings-modal-store'
import { AboutSection } from './app-preferences/about-section'
import { AiAgentsSection } from './app-preferences/ai-agents-section'
import { BrowserSection } from './app-preferences/browser-section'
import { appPrefCategories, appPrefSearchIndex } from './app-preferences/categories'
import { McpServersSection } from './app-preferences/mcp-servers-section'
import { PreferencesSection } from './app-preferences/preferences-section'
import { ProviderSection } from './app-preferences/provider-section'
import { ShortcutsSection } from './app-preferences/shortcuts-section'
import { SkillsSection } from './app-preferences/skills-section'
import { SubagentsSection } from './app-preferences/subagents-section'
import { TerminalProfilesSection } from './app-preferences/terminal-profiles-section'
import { TerminalSection } from './app-preferences/terminal-section'

export function AppPreferencesModal(): React.JSX.Element {
  const isOpen = useSettingsModalStore((state) => state.view === 'app')
  const close = useSettingsModalStore((state) => state.close)
  const appPage = useSettingsModalStore((state) => state.appPage)
  // Read on mount, not at import, so the web/desktop split follows the live runtime.
  const categories = useMemo(() => appPrefCategories(), [])
  const searchIndex = useMemo(() => appPrefSearchIndex(), [])
  const setAppPage = useSettingsModalStore((state) => state.setAppPage)
  const acpTurnTimeoutSecs = useAcpTurnTimeout()
  const acpTurnIdleTimeoutSecs = useAcpTurnIdleTimeout()
  const acpSessionNewTimeoutSecs = useAcpSessionNewTimeout()
  const acpSessionReopenTimeoutSecs = useAcpSessionReopenTimeout()
  const agentIdleWindowSecs = useAgentIdleWindow()
  const updateSetting = useUpdateAppSetting()
  const resetSettings = useResetAppSettings()

  const [availableShells, setAvailableShells] = useState<DetectedShells | null>(null)
  const [isResetDialogOpen, setIsResetDialogOpen] = useState(false)
  const [isResetShortcutsDialogOpen, setIsResetShortcutsDialogOpen] = useState(false)

  // Keyboard shortcuts
  const shortcuts = useKeyboardShortcutsStore((state) => state.shortcuts)
  const updateShortcut = useUpdateShortcut()
  const resetShortcut = useResetShortcut()
  const resetAllShortcuts = useResetAllShortcuts()

  // Profiles offer the detected shells.
  useEffect(() => {
    async function loadShells(): Promise<void> {
      try {
        const result = await shellApi.getAvailableShells()
        if (result.success && result.data) {
          setAvailableShells(result.data)
        }
      } catch {
        // Silently fail - the profile dialog still offers System default
      }
    }
    void loadShells()
  }, [])

  const handleAcpTurnTimeoutChange = async (value: number | null) => {
    await updateSetting('acpTurnTimeoutSecs', value)
    // Push to the Rust core so the next turn picks up the new hard cap.
    try {
      await acpApi.setTurnTimeout(value)
    } catch (error) {
      console.error('Failed to apply ACP turn timeout:', error)
    }
  }

  const handleAcpTurnIdleTimeoutChange = async (value: number | null) => {
    await updateSetting('acpTurnIdleTimeoutSecs', value)
    // Push to the Rust core so the next turn picks up the new idle window.
    try {
      await acpApi.setTurnIdleTimeout(value)
    } catch (error) {
      console.error('Failed to apply ACP turn idle timeout:', error)
    }
  }

  const handleAcpSessionNewTimeoutChange = async (value: number | null) => {
    await updateSetting('acpSessionNewTimeoutSecs', value)
    // Push to the Rust core so the next session/new picks up the new budget.
    try {
      await acpApi.setSessionNewTimeout(value)
    } catch (error) {
      console.error('Failed to apply ACP session/new timeout:', error)
    }
  }

  const handleAcpSessionReopenTimeoutChange = async (value: number | null) => {
    await updateSetting('acpSessionReopenTimeoutSecs', value)
    // Push to the Rust core so the next session/load|resume uses the new budget.
    try {
      await acpApi.setSessionReopenTimeout(value)
    } catch (error) {
      console.error('Failed to apply ACP session reopen timeout:', error)
    }
  }

  const handleAgentIdleWindowChange = async (value: number | null) => {
    const push = queueAgentIdleWindowPush(value)
    await updateSetting('agentIdleWindowSecs', value)
    await push
  }

  const handleResetConfirm = async () => {
    await resetSettings()
    await resetAllShortcuts()
    setIsResetDialogOpen(false)
  }

  const handleResetShortcutsConfirm = async () => {
    await resetAllShortcuts()
    setIsResetShortcutsDialogOpen(false)
  }

  return (
    <>
      <SettingsWindow isOpen={isOpen} onClose={close} title="Preferences">
        <SettingsLayout
          title="Preferences"
          categories={categories}
          searchIndex={searchIndex}
          onClose={close}
          page={appPage ?? categories[0]?.id}
          onPageChange={setAppPage}
        >
          <PreferencesSection />
          <TerminalSection />
          <TerminalProfilesSection availableShells={availableShells} />
          <AiAgentsSection
            acpTurnTimeoutSecs={acpTurnTimeoutSecs}
            acpTurnIdleTimeoutSecs={acpTurnIdleTimeoutSecs}
            acpSessionNewTimeoutSecs={acpSessionNewTimeoutSecs}
            acpSessionReopenTimeoutSecs={acpSessionReopenTimeoutSecs}
            agentIdleWindowSecs={agentIdleWindowSecs}
            handleAcpTurnTimeoutChange={handleAcpTurnTimeoutChange}
            handleAcpTurnIdleTimeoutChange={handleAcpTurnIdleTimeoutChange}
            handleAcpSessionNewTimeoutChange={handleAcpSessionNewTimeoutChange}
            handleAcpSessionReopenTimeoutChange={handleAcpSessionReopenTimeoutChange}
            handleAgentIdleWindowChange={handleAgentIdleWindowChange}
          />
          <SubagentsSection />
          <SkillsSection />
          <ProviderSection />
          <McpServersSection />
          <BrowserSection />
          <ComuSection />
          <ShortcutsSection
            shortcuts={shortcuts}
            updateShortcut={updateShortcut}
            resetShortcut={resetShortcut}
            setIsResetShortcutsDialogOpen={setIsResetShortcutsDialogOpen}
          />
          <AboutSection onResetAll={() => setIsResetDialogOpen(true)} />
        </SettingsLayout>
      </SettingsWindow>

      {/* Reset Confirmation Dialog */}
      <ConfirmDialog
        isOpen={isResetDialogOpen}
        title="Reset all settings?"
        message="Every setting goes back to its default, including keyboard shortcuts. Terminal profiles stay. You cannot undo this."
        confirmLabel="Reset settings"
        cancelLabel="Cancel"
        variant="danger"
        onConfirm={handleResetConfirm}
        onCancel={() => setIsResetDialogOpen(false)}
      />

      {/* Reset Shortcuts Confirmation Dialog */}
      <ConfirmDialog
        isOpen={isResetShortcutsDialogOpen}
        title="Reset all shortcuts?"
        message="Every keyboard shortcut goes back to its default keys. You cannot undo this."
        confirmLabel="Reset shortcuts"
        cancelLabel="Cancel"
        variant="danger"
        onConfirm={handleResetShortcutsConfirm}
        onCancel={() => setIsResetShortcutsDialogOpen(false)}
      />
    </>
  )
}

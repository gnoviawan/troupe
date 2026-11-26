import { renderHook } from '@testing-library/react'
import {
  queueAgentIdleWindowPush,
  useAppSettingsLoader,
  useResetAppSettings,
  useUpdateAppSetting,
  useUpdateAppSettings,
  writeAppSettingsNow
} from '../../../../src/renderer/hooks/use-app-settings'
import { useAppSettingsStore } from '../../../../src/renderer/stores/app-settings-store'
import { useShellLayoutStore } from '../../../../src/renderer/stores/shell-layout-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'
import { emit, idleWindowReplies, readFailures, reads, settingsWatch } from '../record'

/**
 * The settings store with the hooks that read and write `settings/app`
 * (`use-app-settings.ts`): the boot read and its migrations, a synced change
 * from another device, one setting, several, an immediate write, the reset
 * and the ACP timeout and idle-window pushes. `record.test.ts` mocks the host
 * store, `acpApi` and the shared-key watch; each answers from the vector's
 * `context`.
 */

const KEY = 'settings/app'

const settle = async () => {
  for (let tick = 0; tick < 20; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0))
}

const PANEL_ACTIONS = ['setRailExpanded', 'setSidePanelVisible', 'setSshSectionVisible'] as const

/** The reset's panel defaults go to the shell-layout store; record them as effects. */
async function withPanelCalls<T>(run: () => Promise<T>): Promise<T> {
  const saved = Object.fromEntries(
    PANEL_ACTIONS.map((action) => [action, useShellLayoutStore.getState()[action]])
  )
  useShellLayoutStore.setState(
    Object.fromEntries(
      PANEL_ACTIONS.map((action) => [
        action,
        (value: boolean) => emit({ kind: 'shellLayout', action, value })
      ])
    )
  )
  try {
    return await run()
  } finally {
    useShellLayoutStore.setState(saved)
  }
}

let unmountLoader: (() => void) | null = null

async function dispatch({ action, args }: VectorEvent): Promise<unknown> {
  switch (action) {
    case 'load': {
      const [read] = args as [{ data?: unknown; code?: string }]
      reads.delete(KEY)
      readFailures.delete(KEY)
      if (read.code) readFailures.set(KEY, read.code)
      else if (read.data !== undefined) reads.set(KEY, read.data)
      unmountLoader?.()
      unmountLoader = renderHook(() => useAppSettingsLoader()).unmount
      await settle()
      return undefined
    }
    case 'sharedChange':
      settingsWatch.apply?.(args[0])
      return undefined
    case 'updateSetting': {
      const update = renderHook(() => useUpdateAppSetting()).result.current
      await update(args[0] as never, args[1] as never)
      return undefined
    }
    case 'updateSettings': {
      const update = renderHook(() => useUpdateAppSettings()).result.current
      await update(args[0] as never)
      return undefined
    }
    case 'writeNow':
      await writeAppSettingsNow(args[0] as never)
      return undefined
    case 'reset': {
      const reset = renderHook(() => useResetAppSettings()).result.current
      await withPanelCalls(reset)
      return undefined
    }
    case 'setAgentIdleWindow': {
      // `AppPreferences` `handleAgentIdleWindowChange`.
      const secs = args[0] as number | null
      const push = queueAgentIdleWindowPush(secs)
      await renderHook(() => useUpdateAppSetting()).result.current('agentIdleWindowSecs', secs)
      await push
      return undefined
    }
    default:
      throw new Error(`settings recorder has no action ${action}`)
  }
}

export const appSettingsRecorder: StoreRecorder = {
  store: 'app-settings',
  sources: [
    'src/renderer/stores/app-settings-store.ts',
    'src/renderer/hooks/use-app-settings.ts',
    'src/renderer/types/settings.ts',
    'src/renderer/lib/terminal-profiles.ts',
    'src/renderer/lib/themes/theme-appearance.ts'
  ],
  async reset(context) {
    unmountLoader?.()
    unmountLoader = null
    settingsWatch.apply = null
    reads.delete(KEY)
    readFailures.delete(KEY)
    idleWindowReplies.length = 0
    idleWindowReplies.push(...((context.idleWindow as unknown[] | undefined) ?? []))
    useAppSettingsStore.setState(useAppSettingsStore.getInitialState(), true)
    // A push from the previous vector would otherwise still be in line.
    await settle()
  },
  seed(initial) {
    useAppSettingsStore.setState(structuredClone(initial))
  },
  snapshot() {
    const { settings, isLoaded, hostRead } = useAppSettingsStore.getState()
    return { settings, isLoaded, hostRead }
  },
  dispatch
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const idle = (secs: number, source: string) => ({ secs, source })

const PROFILE = { id: 'p1', name: 'Zsh', shell: '/bin/zsh', args: ['-l'], cwd: '', env: { A: '1' } }

export const appSettingsScenarios: Scenario[] = [
  {
    name: 'first_run_reads_nothing_and_keeps_the_defaults',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [ev('load', {})]
  },
  {
    name: 'a_failed_read_keeps_the_defaults_and_warns',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [ev('load', { code: 'STORE_UNAVAILABLE' })]
  },
  {
    name: 'a_stored_record_fills_in_from_the_defaults',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [
      ev('load', {
        data: {
          colorTheme: 'nord',
          appearanceMode: 'light',
          terminalFontSize: 16,
          futureKey: 'kept',
          onboardingCompletedAt: '2026-11-01T00:00:00Z'
        }
      })
    ]
  },
  {
    name: 'old_records_migrate_and_rewrite',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [
      ev('load', {
        data: {
          colorTheme: 'dracula-light',
          terminalUrlOpenMode: 'termul',
          maxTerminalsPerProject: 10,
          orphanDetectionEnabled: true,
          terminalRenderer: 'canvas',
          terminalProfiles: [
            PROFILE,
            { id: 'p1', name: 'dup', shell: '', args: [], cwd: '', env: {} },
            7
          ],
          onboardingCompletedAt: 42
        }
      })
    ]
  },
  {
    name: 'system_and_unknown_appearances_migrate',
    context: { idleWindow: [idle(1800, 'default'), idle(1800, 'default')] },
    steps: [
      ev('load', {
        data: { appearanceMode: 'system', linkOpenMode: 'troupe', terminalUrlOpenMode: 'system' }
      }),
      ev('load', { data: { appearanceMode: 'sepia' } })
    ]
  },
  {
    name: 'the_hosts_idle_window_replaces_the_saved_one',
    context: {
      idleWindow: [
        idle(3600, 'setting'),
        idle(1800, 'setting'),
        idle(900, 'env'),
        { reject: 'offline' }
      ]
    },
    steps: [
      ev('load', { data: { appearanceMode: 'dark' } }),
      ev('load', { data: { appearanceMode: 'dark', agentIdleWindowSecs: 7200 } }),
      ev('load', { data: { appearanceMode: 'dark' } }),
      ev('load', { data: { appearanceMode: 'dark' } })
    ]
  },
  {
    name: 'edits_write_the_whole_record',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [
      ev('load', { data: { appearanceMode: 'dark', terminalProfiles: [PROFILE] } }),
      ev('updateSetting', 'terminalFontSize', 18),
      ev('updateSetting', 'colorTheme', 'nord'),
      ev('updateSettings', { colorTheme: 'termul', appearanceMode: 'light' }),
      ev('writeNow', { onboardingCompletedAt: '2026-11-25T10:00:00Z' }),
      ev('updateSetting', 'acpTurnTimeoutSecs', 3600)
    ]
  },
  {
    name: 'a_synced_change_replaces_the_settings_and_keeps_the_read',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [
      ev('load', { data: { appearanceMode: 'dark' } }),
      ev('sharedChange', { exists: true, data: { colorTheme: 'nord-light', uiZoomLevel: 1.2 } }),
      ev('sharedChange', { exists: false }),
      ev('sharedChange', {
        exists: true,
        data: { appearanceMode: 'light', terminalRenderer: 'canvas' }
      })
    ]
  },
  {
    name: 'reset_keeps_profiles_and_onboarding',
    context: { idleWindow: [idle(1800, 'default')] },
    steps: [
      ev('load', {
        data: {
          appearanceMode: 'light',
          colorTheme: 'nord',
          acpTurnTimeoutSecs: 3600,
          terminalProfiles: [PROFILE],
          onboardingCompletedAt: '2026-11-01T00:00:00Z'
        }
      }),
      ev('reset')
    ]
  },
  {
    name: 'idle_window_choice_pushes_then_writes',
    context: { idleWindow: [idle(1800, 'default'), { failPush: 'host refused' }] },
    steps: [
      ev('load', { data: { appearanceMode: 'dark' } }),
      ev('setAgentIdleWindow', 3600),
      ev('setAgentIdleWindow', null),
      ev('setAgentIdleWindow', 900)
    ]
  }
]

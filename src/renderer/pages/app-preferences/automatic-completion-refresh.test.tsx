import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { type CompletionContext, CompletionScheduler } from '@/lib/command-lifecycle/completion'
import { useAppSettingsStore } from '@/stores/app-settings-store'
import { DEFAULT_APP_SETTINGS } from '@/types/settings'
import { TerminalSection } from './terminal-section'

vi.mock('@/lib/api', () => ({
  shellApi: { getAvailableShells: vi.fn().mockResolvedValue({ success: false }) },
  persistenceApi: { writeDebounced: vi.fn().mockResolvedValue({ success: true }) },
  acpApi: {}
}))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

beforeEach(() => {
  useAppSettingsStore.setState({ settings: { ...DEFAULT_APP_SETTINGS }, isLoaded: true })
})

afterEach(() => {
  vi.useRealTimers()
})

it('refresh defaults on', () => {
  expect(DEFAULT_APP_SETTINGS.automaticCompletionRefresh).toBe(true)
})

it('describes an immediate effect, not a new-terminal-only one', () => {
  // The scheduler reads the setting live per request (bind-session passes a
  // getter over the store), so the copy must not claim new-terminals-only.
  render(<TerminalSection />)

  const row = within(document.getElementById('terminal-automatic-completion-refresh-description')!)
  expect(row.getByText(/Applies immediately/)).toBeTruthy()
  expect(row.queryByText(/new terminals/)).toBeNull()
})

it('switch renders on and writes the setting', async () => {
  render(<TerminalSection />)

  const toggle = screen.getByRole('switch', {
    name: 'Automatic completion refresh',
    hidden: true
  })
  expect(toggle).toHaveAttribute('aria-checked', 'true')

  fireEvent.click(toggle)
  await waitFor(() =>
    expect(useAppSettingsStore.getState().settings.automaticCompletionRefresh).toBe(false)
  )
})

it('scheduler reads the setting', async () => {
  vi.useFakeTimers()
  const requests: unknown[] = []
  const host = {
    requestCompletion(_terminalId: string, request: unknown) {
      requests.push(request)
      return new Promise(() => {})
    },
    cancelCompletion() {
      return Promise.resolve('Accepted' as const)
    }
  }
  const clock = {
    setTimeout: (handler: () => void, ms: number) => setTimeout(handler, ms),
    clearTimeout: (id: unknown) => clearTimeout(id as ReturnType<typeof setTimeout>)
  }
  const settings = { automaticCompletionRefresh: false }
  const scheduler = new CompletionScheduler({
    host,
    createRequestId: () => 'A',
    settings,
    clock
  })
  const context: CompletionContext = {
    terminalId: 't1',
    sessionId: 's1',
    mode: 'Editor',
    phase: 'AtPrompt',
    draft: 'git sta',
    bufferRevision: 4,
    cursor: 7,
    promptGeneration: 3,
    cwd: '/repo'
  }

  scheduler.updateContext(context)
  await vi.advanceTimersByTimeAsync(100)

  expect(requests).toHaveLength(0)
})

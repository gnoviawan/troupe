/**
 * Unit tests for Troupe Comu UI components (ComuAgentIndicator, ComuConsentCard,
 * ComuConsentCardHost, ComuAppPolicy) and policy/consent/Stop facades across
 * desktop (`invoke`) and remote web (`fetch` / `acp-transport`).
 */

import type { AppComuPolicy, ComuConsentTicket, ProjectComuPolicy } from '@shared/types/comu.types'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const {
  mockFetch,
  mockIsTauriContext,
  mockInvoke,
  mockRespondPermission,
  mockToastError,
  mockLogFrontendError
} = vi.hoisted(() => ({
  mockFetch: vi.fn(),
  mockIsTauriContext: vi.fn(),
  mockInvoke: vi.fn(),
  mockRespondPermission: vi.fn(),
  mockToastError: vi.fn(),
  mockLogFrontendError: vi.fn(async () => {})
}))

vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: mockIsTauriContext }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: mockLogFrontendError }))
vi.mock('@/lib/acp-transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/acp-transport')>()),
  getAcpTransport: () => ({ respondPermission: mockRespondPermission })
}))
vi.mock('sonner', () => ({ toast: { error: mockToastError } }))

import { ComuAppPolicy } from '@/components/settings/ComuPolicySettings'
import { readPendingPrompts } from '@/lib/acp-transport/pending-permissions'
import {
  answerComuConsent,
  comuPolicyApi,
  comuPolicyFailure,
  editAppComuPolicy,
  editProjectComuPolicy,
  onComuPolicyChanged,
  parseComuConsentTicket,
  publishComuPolicyChange,
  stopComuAgent
} from '@/lib/comu-policy-api'
import { useComuControlStore } from '@/stores/comu-control-store'
import {
  COMU_STOP_LABEL,
  ComuAgentIndicator,
  ComuConsentCard,
  ComuConsentCardHost
} from './ComuConsentCardHost'

const SAMPLE_TICKET: ComuConsentTicket = {
  requestId: 'host:test-comu-1',
  sessionId: 's1',
  appName: 'Calculator',
  appId: 'com.apple.calculator',
  action: 'drive_app',
  toolName: 'comu_click',
  options: ['allow_session', 'allow_project', 'deny']
}

function jsonResponse(body: unknown): Response {
  return { ok: true, status: 200, statusText: 'OK', json: () => Promise.resolve(body) } as Response
}

function makeAppPolicy(overrides: Partial<AppComuPolicy> = {}): AppComuPolicy {
  return {
    revision: 0,
    readOnly: false,
    enabled: true,
    cursorEnabled: true,
    askBeforeDrivingApps: true,
    askBeforeForegroundDesktop: true,
    askBeforeClipboardRead: true,
    allowedApps: [],
    ...overrides
  }
}

function makeProjectPolicy(
  projectId: string,
  overrides: Partial<ProjectComuPolicy> = {}
): ProjectComuPolicy {
  return {
    projectId,
    revision: 0,
    readOnly: false,
    allowedApps: [],
    ...overrides
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  mockIsTauriContext.mockReturnValue(true)
  mockInvoke.mockResolvedValue({ success: true, data: true })
  vi.stubGlobal('fetch', mockFetch)
  useComuControlStore.setState({
    activeSessions: {},
    stoppedSessions: {},
    activeSeqs: {},
    stopSeqs: {},
    pendingConsents: {},
    hostedSessionIds: new Set()
  })
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  cleanup()
})

describe('ComuAgentIndicator', () => {
  it('renders nothing while the session is not using Computer Use', () => {
    useComuControlStore.setState({ activeSessions: { other: true } })
    const { container } = render(<ComuAgentIndicator sessionId="s1" />)
    expect(container).toBeEmptyDOMElement()
  })

  it('shows the active indicator and invokes comu_stop when Stop is clicked', async () => {
    useComuControlStore.setState({ activeSessions: { s1: true } })
    render(<ComuAgentIndicator sessionId="s1" />)
    expect(screen.getByRole('status')).toHaveTextContent(
      'The agent is using Computer Use on this machine.'
    )
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: COMU_STOP_LABEL }))
    })
    expect(mockInvoke).toHaveBeenCalledWith('comu_stop', { sessionId: 's1' })
    expect(mockToastError).not.toHaveBeenCalled()
  })

  it('renders the stopped message without the Stop button once stopped', () => {
    useComuControlStore.setState({
      activeSessions: { s1: true },
      stoppedSessions: { s1: true }
    })
    render(<ComuAgentIndicator sessionId="s1" />)
    expect(screen.getByRole('status')).toHaveTextContent(
      'Computer Use stopped until your next message.'
    )
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  it('shows an error toast if stopping fails', async () => {
    mockInvoke.mockResolvedValue({ success: false, code: 'INTERNAL_ERROR', error: 'failed' })
    useComuControlStore.setState({ activeSessions: { s1: true } })
    render(<ComuAgentIndicator sessionId="s1" />)
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: COMU_STOP_LABEL }))
    })
    expect(mockToastError).toHaveBeenCalledWith('Could not stop Computer Use. Try again.')
  })
})

describe('ComuConsentCard & ComuConsentCardHost', () => {
  it('keeps options disabled during the 750ms arming floor and dispatches after arming', async () => {
    vi.useFakeTimers()
    mockInvoke.mockResolvedValue({ success: true, data: null })
    useComuControlStore.getState().addPendingConsent(SAMPLE_TICKET)

    render(<ComuConsentCard consent={SAMPLE_TICKET} />)
    expect(screen.getByText('Allow Computer Use in Calculator?')).toBeInTheDocument()
    expect(screen.getByText('com.apple.calculator')).toBeInTheDocument()

    const allowBtn = screen.getByRole('button', { name: 'Allow for this chat' })
    expect(allowBtn).toBeDisabled()

    await act(async () => {
      vi.advanceTimersByTime(800)
    })
    expect(allowBtn).not.toBeDisabled()

    await act(async () => {
      fireEvent.click(allowBtn)
    })
    expect(mockInvoke).toHaveBeenCalledWith('browser_ticket_respond', {
      requestId: SAMPLE_TICKET.requestId,
      optionId: 'allow_session'
    })
  })

  it('renders unhosted consent cards inside a viewport-bounded scrollable container', () => {
    useComuControlStore.getState().addPendingConsent(SAMPLE_TICKET)
    render(<ComuConsentCardHost />)
    const host = screen.getByTestId('comu-consent-card-host')
    expect(host.className).toContain('max-h-[calc(100dvh-2rem)]')
    expect(host.className).toContain('overflow-y-auto')
  })

  it('does not reinsert a closed ticket when an in-flight answer later fails, while restoring live tickets', async () => {
    let resolveAnswer!: (res: { success: boolean; code?: string; error?: string }) => void
    mockInvoke.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveAnswer = resolve
        })
    )

    const store = useComuControlStore.getState()
    store.addPendingConsent(SAMPLE_TICKET)
    store.respondConsent(SAMPLE_TICKET.requestId, 'allow_session', 'claude')
    expect(useComuControlStore.getState().pendingConsents[SAMPLE_TICKET.requestId]).toBeUndefined()

    // Ticket closes on host while answer is still in flight.
    store.removePendingConsent(SAMPLE_TICKET.requestId)
    await act(async () => {
      resolveAnswer({ success: false, code: 'TICKET_TOO_EARLY', error: 'too early' })
    })
    expect(useComuControlStore.getState().pendingConsents[SAMPLE_TICKET.requestId]).toBeUndefined()
    expect(mockToastError).not.toHaveBeenCalled()

    // Live ticket that was NOT closed IS restored on retryable failure.
    mockInvoke.mockResolvedValueOnce({
      success: false,
      code: 'TICKET_TOO_EARLY',
      error: 'too early'
    })
    const liveTicket = { ...SAMPLE_TICKET, requestId: 'host:test-comu-2' }
    store.addPendingConsent(liveTicket)
    await act(async () => {
      store.respondConsent(liveTicket.requestId, 'allow_session', 'claude')
    })
    expect(useComuControlStore.getState().pendingConsents[liveTicket.requestId]).toEqual(liveTicket)
    expect(mockToastError).toHaveBeenCalledTimes(1)
  })
})

describe('Comu reconnect state & permissions error retry', () => {
  it('restores active and stopped flags from subscribe/snapshot comuState and ignores older seqs', () => {
    const prompts = readPendingPrompts('s1', {
      comuState: { active: true, stopped: true, seq: 4 }
    })
    expect(prompts.comuState).toEqual({ active: true, stopped: true, seq: 4 })

    const store = useComuControlStore.getState()
    store.installSessionState('s1', prompts.comuState!)
    expect(useComuControlStore.getState().activeSessions.s1).toBe(true)
    expect(useComuControlStore.getState().stoppedSessions.s1).toBe(true)

    // Older seq does not overwrite newer state.
    store.installSessionState('s1', { active: false, stopped: false, seq: 3 })
    expect(useComuControlStore.getState().activeSessions.s1).toBe(true)
    expect(useComuControlStore.getState().stoppedSessions.s1).toBe(true)

    // Newer seq clears Stop and deactivates cleanly.
    store.installSessionState('s1', { active: false, stopped: false, seq: 5 })
    expect(useComuControlStore.getState().activeSessions.s1).toBeUndefined()
    expect(useComuControlStore.getState().stoppedSessions.s1).toBeUndefined()
  })

  it('surfaces permission check failures with a working Recheck retry button and logs through log-api', async () => {
    mockInvoke
      .mockResolvedValueOnce({ success: true, data: makeAppPolicy() })
      .mockResolvedValueOnce({
        success: false,
        code: 'INTERNAL_ERROR',
        error: 'permission probe temporarily unavailable'
      })
      .mockResolvedValueOnce({
        success: true,
        data: {
          platform: 'macos',
          accessibilityGranted: true,
          screenRecordingGranted: true,
          displayAvailable: true,
          ready: true,
          summary: 'Ready'
        }
      })

    render(<ComuAppPolicy />)
    await waitFor(() => {
      expect(screen.getByText('System access status unavailable')).toBeInTheDocument()
    })
    expect(mockLogFrontendError).toHaveBeenCalled()

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Recheck/i }))
    })
    await waitFor(() => {
      expect(screen.getByText('System access ready')).toBeInTheDocument()
    })
  })
})

describe('comuPolicyApi transport routing and CAS retries', () => {
  it('desktop: routes all 6 methods through comu_* Tauri commands', async () => {
    mockIsTauriContext.mockReturnValue(true)
    mockInvoke.mockResolvedValue({ success: true, data: makeAppPolicy() })

    await comuPolicyApi.getProject('proj-1')
    await comuPolicyApi.getApp()
    await comuPolicyApi.applyProject('proj-1', 2, { op: 'add_app', app: 'Calculator' })
    await comuPolicyApi.applyApp(3, { op: 'set_cursor_enabled', enabled: false })
    await comuPolicyApi.getPermissions()
    await comuPolicyApi.requestPermission('accessibility')

    expect(mockInvoke.mock.calls).toEqual([
      ['comu_policy_get_project', { projectId: 'proj-1' }],
      ['comu_policy_get_app', undefined],
      [
        'comu_policy_apply_project',
        { projectId: 'proj-1', expectedRevision: 2, op: { op: 'add_app', app: 'Calculator' } }
      ],
      [
        'comu_policy_apply_app',
        { expectedRevision: 3, op: { op: 'set_cursor_enabled', enabled: false } }
      ],
      ['comu_permissions_status', undefined],
      ['comu_request_permission', { kind: 'accessibility' }]
    ])
    expect(mockFetch).not.toHaveBeenCalled()
  })

  it('web: routes all 6 methods through /host/comu/* HTTP endpoints', async () => {
    mockIsTauriContext.mockReturnValue(false)
    mockFetch.mockImplementation(async () => jsonResponse({ success: true, data: makeAppPolicy() }))

    await comuPolicyApi.getProject('proj 1')
    await comuPolicyApi.getApp()
    await comuPolicyApi.applyProject('proj-1', 1, { op: 'remove_app', app: 'Calculator' })
    await comuPolicyApi.applyApp(0, { op: 'set_enabled', enabled: false })
    await comuPolicyApi.getPermissions()
    await comuPolicyApi.requestPermission('screen_recording')

    const calls = mockFetch.mock.calls.map(([url, init]) => [
      String(url).replace(/^https?:\/\/[^/]+/, ''),
      (init as RequestInit)?.method ?? 'GET',
      (init as RequestInit)?.body
    ])
    expect(calls).toEqual([
      ['/host/comu/policy/project?projectId=proj%201', 'GET', undefined],
      ['/host/comu/policy/app', 'GET', undefined],
      [
        '/host/comu/policy/project',
        'POST',
        JSON.stringify({
          projectId: 'proj-1',
          expectedRevision: 1,
          op: { op: 'remove_app', app: 'Calculator' }
        })
      ],
      [
        '/host/comu/policy/app',
        'POST',
        JSON.stringify({ expectedRevision: 0, op: { op: 'set_enabled', enabled: false } })
      ],
      ['/host/comu/permissions', 'GET', undefined],
      ['/host/comu/permissions/request', 'POST', JSON.stringify({ kind: 'screen_recording' })]
    ])
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('reloads and retries on revision_conflict, parses refusals, and handles consent/Stop', async () => {
    mockIsTauriContext.mockReturnValue(true)
    let appConflictSent = false
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'comu_policy_apply_app' && !appConflictSent) {
        appConflictSent = true
        return {
          success: false,
          code: 'CONFLICT',
          error: 'revision_conflict: policy changed on another surface (current revision 4)'
        }
      }
      if (cmd === 'comu_policy_get_app') {
        return { success: true, data: makeAppPolicy({ revision: 4 }) }
      }
      if (cmd === 'comu_policy_apply_app') {
        expect(args?.expectedRevision).toBe(4)
        return { success: true, data: makeAppPolicy({ revision: 5, cursorEnabled: false }) }
      }
      if (cmd === 'comu_policy_get_project') {
        return { success: true, data: makeProjectPolicy('p1', { revision: 2 }) }
      }
      if (cmd === 'comu_policy_apply_project') {
        return {
          success: true,
          data: makeProjectPolicy('p1', { revision: 3, allowedApps: ['Calculator'] })
        }
      }
      throw new Error(`unexpected command ${cmd}`)
    })

    const appRes = await editAppComuPolicy(
      { op: 'set_cursor_enabled', enabled: false },
      makeAppPolicy({ revision: 1 })
    )
    expect(appRes.success && appRes.data.revision).toBe(5)

    const projRes = await editProjectComuPolicy('p1', { op: 'add_app', app: 'Calculator' })
    expect(projRes.success && projRes.data.allowedApps).toEqual(['Calculator'])

    expect(
      comuPolicyFailure({
        success: false,
        code: 'INVALID_PARAMS',
        error: 'invalid_app_id: Troupe itself cannot be added to Allowed Apps'
      }).reason
    ).toBe('invalid_app_id')

    const seen: unknown[] = []
    const unsub = onComuPolicyChanged((change) => seen.push(change))
    publishComuPolicyChange({ scope: 'app', revision: 2 })
    unsub()
    expect(seen).toEqual([{ scope: 'app', revision: 2 }])

    expect(parseComuConsentTicket(null)).toBeNull()
    expect(
      parseComuConsentTicket({
        requestId: 'host:abcd-1234',
        sessionId: 'sess-9',
        options: [{ optionId: 'allow_session' }, { optionId: 'deny' }],
        _meta: {
          termul: {
            origin_kind: 'comu',
            comu: {
              appName: 'Simulator',
              appId: 'com.apple.iphonesimulator',
              action: 'drive_app',
              toolName: 'comu_click'
            }
          }
        }
      })?.appId
    ).toBe('com.apple.iphonesimulator')

    mockIsTauriContext.mockReturnValue(false)
    mockRespondPermission.mockResolvedValueOnce(undefined)
    mockFetch.mockResolvedValueOnce(jsonResponse({ success: true, data: true }))
    expect(await answerComuConsent({ requestId: 'host:2' }, 'allow_project', 'claude')).toBe('ok')
    expect(await stopComuAgent('s2')).toBe(true)
  })
})

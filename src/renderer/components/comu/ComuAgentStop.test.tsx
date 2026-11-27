/**
 * Unit tests for ComuAgentIndicator (live Stop button) and ComuConsentCard
 * (750ms anti-clickjacking arming floor + consent option dispatch).
 */

import type { ComuConsentTicket } from '@shared/types/comu.types'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { mockStop, mockAnswer, mockToastError } = vi.hoisted(() => ({
  mockStop: vi.fn(),
  mockAnswer: vi.fn(),
  mockToastError: vi.fn()
}))

vi.mock('@/lib/comu-consent-api', () => ({
  stopComuAgent: mockStop,
  answerComuConsent: mockAnswer
}))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(async () => {}) }))
vi.mock('sonner', () => ({ toast: { error: mockToastError } }))

import { useComuControlStore } from '@/stores/comu-control-store'
import { COMU_STOP_LABEL, ComuAgentIndicator } from './ComuAgentStop'
import { ComuConsentCard } from './ComuConsentCard'

const SAMPLE_TICKET: ComuConsentTicket = {
  requestId: 'host:test-comu-1',
  sessionId: 's1',
  appName: 'Calculator',
  appId: 'com.apple.calculator',
  action: 'drive_app',
  toolName: 'comu_click',
  options: ['allow_session', 'allow_project', 'deny']
}

beforeEach(() => {
  mockStop.mockReset().mockResolvedValue(true)
  mockAnswer.mockReset().mockResolvedValue('ok')
  mockToastError.mockReset()
  useComuControlStore.setState({
    activeSessions: {},
    stoppedSessions: {},
    pendingConsents: {},
    hostedSessionIds: new Set()
  })
})

afterEach(() => {
  vi.useRealTimers()
  cleanup()
})

describe('ComuAgentIndicator', () => {
  it('renders nothing while the session is not using Computer Use', () => {
    useComuControlStore.setState({ activeSessions: { other: true } })
    const { container } = render(<ComuAgentIndicator sessionId="s1" />)
    expect(container).toBeEmptyDOMElement()
  })

  it('shows the active indicator and invokes stopComuAgent when Stop is clicked', async () => {
    useComuControlStore.setState({ activeSessions: { s1: true } })
    render(<ComuAgentIndicator sessionId="s1" />)
    expect(screen.getByRole('status')).toHaveTextContent(
      'The agent is using Computer Use on this machine.'
    )
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: COMU_STOP_LABEL }))
    })
    expect(mockStop).toHaveBeenCalledWith('s1')
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
    mockStop.mockResolvedValue(false)
    useComuControlStore.setState({ activeSessions: { s1: true } })
    render(<ComuAgentIndicator sessionId="s1" />)
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: COMU_STOP_LABEL }))
    })
    expect(mockToastError).toHaveBeenCalledWith('Could not stop Computer Use. Try again.')
  })
})

describe('ComuConsentCard', () => {
  it('keeps options disabled during the 750ms arming floor and dispatches after arming', async () => {
    vi.useFakeTimers()
    useComuControlStore.setState({
      pendingConsents: { [SAMPLE_TICKET.requestId]: SAMPLE_TICKET }
    })

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
    expect(mockAnswer).toHaveBeenCalledWith(SAMPLE_TICKET, 'allow_session', '')
  })
})

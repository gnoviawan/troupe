import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mockSessionIndexEntry } from '@/lib/test-utils/acp'
import { useAcpStore } from '@/stores/acp-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'
import { QuickChatsGroup } from './QuickChatsGroup'

const rename = vi.hoisted(() => vi.fn())
const openHistory = vi.fn()
const deleteHistory = vi.fn()

vi.mock('@/lib/acp-api', () => ({
  acpRenameQuickChat: (...args: unknown[]) => rename(...args)
}))

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

function chat(id: string, title: string, createdAt: number, kind?: 'quick') {
  return mockSessionIndexEntry({
    id,
    title,
    createdAt,
    projectId: kind === 'quick' ? '__unassigned__' : 'p1',
    cwd: kind === 'quick' ? `/home/me/.troupe/quick-chats/${id}` : '/work',
    kind
  })
}

describe('QuickChatsGroup', () => {
  beforeEach(() => {
    rename.mockReset()
    rename.mockResolvedValue(undefined)
    openHistory.mockReset()
    deleteHistory.mockReset()
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    useAcpStore.setState({
      sessionIndex: [
        chat('old', 'Older notes', 10, 'quick'),
        chat('new', 'Newer notes', 20, 'quick'),
        chat('legacy', 'Legacy unassigned', 30)
      ],
      openHistorySession: openHistory,
      deleteHistorySession: deleteHistory
    })
  })

  it('marks the open quick chat as the active row', () => {
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'new' } })
    render(<QuickChatsGroup />)
    expect(screen.getByRole('button', { name: 'Newer notes' })).toHaveAttribute(
      'aria-current',
      'true'
    )
    expect(screen.getByRole('button', { name: 'Older notes' })).not.toHaveAttribute('aria-current')
  })

  it('lists quick chats newest first and hides legacy unassigned sessions', () => {
    render(<QuickChatsGroup />)
    const titles = screen
      .getAllByRole('button', { name: /^(Newer|Older) notes$/ })
      .map((button) => button.textContent)
    expect(titles).toEqual(['Newer notes', 'Older notes'])
    expect(screen.queryByText('Legacy unassigned')).not.toBeInTheDocument()
  })

  it('renames a quick chat through the local alias', async () => {
    render(<QuickChatsGroup />)
    fireEvent.click(screen.getByRole('button', { name: 'Rename Newer notes' }))
    const input = screen.getByRole('textbox', { name: 'Rename Newer notes' })
    fireEvent.change(input, { target: { value: 'Renamed chat' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(rename).toHaveBeenCalledWith('new', 'Renamed chat')
  })

  it('warns that delete removes the chat folder, then deletes the session', () => {
    render(<QuickChatsGroup />)
    fireEvent.click(screen.getByRole('button', { name: 'Delete Newer notes' }))
    expect(
      screen.getByText(/Files the agent created in this chat’s folder are deleted too/)
    ).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    expect(deleteHistory).toHaveBeenCalledWith('new')
  })
})

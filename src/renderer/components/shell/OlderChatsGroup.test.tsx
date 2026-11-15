import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mockSessionIndexEntry } from '@/lib/test-utils/acp'
import { useAcpStore } from '@/stores/acp-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'
import { OlderChatsGroup } from './OlderChatsGroup'

const openHistory = vi.fn()
const deleteHistory = vi.fn()

function chat(id: string, title: string, createdAt: number, kind?: 'quick') {
  return mockSessionIndexEntry({
    id,
    title,
    createdAt,
    projectId: '__unassigned__',
    cwd: kind === 'quick' ? `/home/me/.troupe/quick-chats/${id}` : '/tmp/notes',
    kind
  })
}

describe('OlderChatsGroup', () => {
  beforeEach(() => {
    openHistory.mockReset()
    deleteHistory.mockReset()
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    useAcpStore.setState({
      sessionIndex: [
        chat('old', 'Notes from /tmp', 10),
        chat('quick', 'A quick chat', 20, 'quick')
      ],
      openHistorySession: openHistory,
      deleteHistorySession: deleteHistory
    })
  })

  it('lists legacy unassigned chats and not quick chats', () => {
    render(<OlderChatsGroup />)
    expect(screen.getByText('Older chats')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Notes from /tmp' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'A quick chat' })).not.toBeInTheDocument()
  })

  it('opens the chat read-only', () => {
    render(<OlderChatsGroup />)
    fireEvent.click(screen.getByRole('button', { name: 'Notes from /tmp' }))
    expect(useQuickChatStore.getState().view).toEqual({
      mode: 'chat',
      sessionId: 'old',
      readOnly: true
    })
    expect(openHistory).toHaveBeenCalledWith('old')
  })

  it('confirms delete of the record and does not mention the folder being deleted', () => {
    render(<OlderChatsGroup />)
    fireEvent.click(screen.getByRole('button', { name: 'Delete Notes from /tmp' }))
    expect(screen.getByText(/Files in the original folder are kept/)).toBeInTheDocument()
    expect(screen.queryByText(/folder are deleted/)).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    expect(deleteHistory).toHaveBeenCalledWith('old')
  })
})

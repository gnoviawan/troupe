import { useMemo, useState } from 'react'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { ChevronDown, ChevronRight, MessageSquare, Pencil, Trash2 } from '@/components/icons'
import { RAIL_ICON_BOX_CLASS, RAIL_ROW_INSET_CLASS } from '@/components/shell/rail-layout'
import { FIELD_FOCUS_CLASS } from '@/components/ui/panel-styles'
import { acpRenameQuickChat } from '@/lib/acp-api'
import { quickSessions, UNTITLED_HOST_TITLE } from '@/lib/acp-history-persistence'
import { logFrontendError } from '@/lib/log-api'
import { cn } from '@/lib/utils'
import { useAcpStore } from '@/stores/acp-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'

/**
 * Collapsible Quick chats group for the shell rail. Newest chat first.
 * Each row opens the chat, and can rename or delete it.
 */
export function QuickChatsGroup(): React.JSX.Element {
  const sessionIndex = useAcpStore((state) => state.sessionIndex)
  const deleteHistorySession = useAcpStore((state) => state.deleteHistorySession)
  const openHistorySession = useAcpStore((state) => state.openHistorySession)
  const chats = useMemo(() => {
    return quickSessions(sessionIndex)
      .slice()
      .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0))
  }, [sessionIndex])
  const [open, setOpen] = useState(true)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const openId = useQuickChatStore((state) =>
    state.view.mode === 'chat' ? state.view.sessionId : null
  )
  const deleteTarget = chats.find((chat) => chat.id === deleteId) ?? null

  const saveRename = async (sessionId: string): Promise<void> => {
    const title = draft.trim()
    setEditingId(null)
    if (!title) return
    try {
      await acpRenameQuickChat(sessionId, title)
      useAcpStore.setState((state) => ({
        sessionIndex: state.sessionIndex.map((entry) =>
          entry.id === sessionId ? { ...entry, title } : entry
        ),
        sessions: state.sessions[sessionId]
          ? {
              ...state.sessions,
              [sessionId]: { ...state.sessions[sessionId], title }
            }
          : state.sessions
      }))
    } catch (error) {
      void logFrontendError({
        level: 'error',
        source: 'quick-chat.rename',
        message: `Could not rename quick chat ${sessionId}: ${error instanceof Error ? error.message : String(error)}`
      })
    }
  }

  return (
    <div className="flex shrink-0 flex-col pb-1" data-testid="quick-chats-group">
      <button
        type="button"
        className={cn('flex h-8 w-full items-center gap-1 text-left', RAIL_ROW_INSET_CLASS)}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span className="label-panel min-w-0 truncate">Quick chats</span>
      </button>
      {open &&
        (chats.length === 0 ? (
          <p className="px-3 py-1 text-xs text-muted-foreground">No quick chats yet</p>
        ) : (
          <ul className="flex flex-col">
            {chats.map((chat) => {
              const title = chat.title || UNTITLED_HOST_TITLE
              const isActive = openId === chat.id
              return (
                <li key={chat.id} className="group/quick flex items-center pr-1">
                  {editingId === chat.id ? (
                    <input
                      aria-label={`Rename ${title}`}
                      className={cn(
                        'mx-2 h-7 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-xs text-foreground pointer-coarse:text-base',
                        FIELD_FOCUS_CLASS
                      )}
                      value={draft}
                      ref={(node) => node?.focus()}
                      onChange={(event) => setDraft(event.target.value)}
                      onBlur={() => void saveRename(chat.id)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') void saveRename(chat.id)
                        if (event.key === 'Escape') setEditingId(null)
                      }}
                    />
                  ) : (
                    <button
                      type="button"
                      aria-current={isActive ? 'true' : undefined}
                      data-active={isActive ? 'true' : undefined}
                      className={cn(
                        'flex h-8 min-w-0 flex-1 items-center rounded-lg text-left text-xs text-sidebar-foreground hover:bg-sidebar-accent',
                        isActive && 'bg-sidebar-accent text-sidebar-accent-foreground'
                      )}
                      onClick={() => {
                        useQuickChatStore.getState().openChat(chat.id)
                        void openHistorySession(chat.id)
                      }}
                    >
                      <span className={RAIL_ICON_BOX_CLASS}>
                        <MessageSquare size={14} />
                      </span>
                      <span className="min-w-0 flex-1 truncate">{title}</span>
                    </button>
                  )}
                  {editingId !== chat.id && (
                    <span className="flex shrink-0 opacity-0 group-hover/quick:opacity-100 group-focus-within/quick:opacity-100">
                      <button
                        type="button"
                        aria-label={`Rename ${title}`}
                        className="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-foreground"
                        onClick={() => {
                          setEditingId(chat.id)
                          setDraft(title)
                        }}
                      >
                        <Pencil size={12} />
                      </button>
                      <button
                        type="button"
                        aria-label={`Delete ${title}`}
                        className="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-foreground"
                        onClick={() => setDeleteId(chat.id)}
                      >
                        <Trash2 size={12} />
                      </button>
                    </span>
                  )}
                </li>
              )
            })}
          </ul>
        ))}
      <ConfirmDialog
        isOpen={deleteTarget !== null}
        title="Delete quick chat"
        message={
          deleteTarget
            ? `Delete “${deleteTarget.title || UNTITLED_HOST_TITLE}”? Files the agent created in this chat’s folder are deleted too.`
            : ''
        }
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => {
          if (!deleteTarget) return
          const id = deleteTarget.id
          setDeleteId(null)
          void deleteHistorySession(id)
          const view = useQuickChatStore.getState().view
          if (view.mode === 'chat' && view.sessionId === id) {
            useQuickChatStore.getState().close()
          }
        }}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  )
}

import { useMemo, useState } from 'react'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { ChevronDown, ChevronRight, MessageSquare, Trash2 } from '@/components/icons'
import { RAIL_ICON_BOX_CLASS, RAIL_ROW_INSET_CLASS } from '@/components/shell/rail-layout'
import { olderChats, UNTITLED_HOST_TITLE } from '@/lib/acp-history-persistence'
import { cn } from '@/lib/utils'
import { useAcpStore } from '@/stores/acp-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'

/**
 * Legacy chats that have no project. They open read-only. Delete removes the
 * session record and leaves the original folder in place.
 */
export function OlderChatsGroup(): React.JSX.Element | null {
  const sessionIndex = useAcpStore((state) => state.sessionIndex)
  const deleteHistorySession = useAcpStore((state) => state.deleteHistorySession)
  const openHistorySession = useAcpStore((state) => state.openHistorySession)
  const chats = useMemo(() => {
    return olderChats(sessionIndex)
      .slice()
      .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0))
  }, [sessionIndex])
  const [open, setOpen] = useState(true)
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const deleteTarget = chats.find((chat) => chat.id === deleteId) ?? null
  const openId = useQuickChatStore((state) =>
    state.view.mode === 'chat' && state.view.readOnly ? state.view.sessionId : null
  )

  if (chats.length === 0) return null

  return (
    <div className="flex shrink-0 flex-col pb-1" data-testid="older-chats-group">
      <button
        type="button"
        className={cn('flex h-8 w-full items-center gap-1 text-left', RAIL_ROW_INSET_CLASS)}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span className="label-panel min-w-0 truncate">Older chats</span>
      </button>
      {open && (
        <ul className="flex flex-col">
          {chats.map((chat) => {
            const title = chat.title || UNTITLED_HOST_TITLE
            const isActive = openId === chat.id
            return (
              <li key={chat.id} className="group/older flex items-center pr-1">
                <button
                  type="button"
                  aria-current={isActive ? 'true' : undefined}
                  className={cn(
                    'flex h-8 min-w-0 flex-1 items-center rounded-lg text-left text-xs text-sidebar-foreground hover:bg-sidebar-accent',
                    isActive && 'bg-sidebar-accent text-sidebar-accent-foreground'
                  )}
                  onClick={() => {
                    useQuickChatStore.getState().openOlderChat(chat.id)
                    void openHistorySession(chat.id)
                  }}
                >
                  <span className={RAIL_ICON_BOX_CLASS}>
                    <MessageSquare size={14} />
                  </span>
                  <span className="min-w-0 flex-1 truncate">{title}</span>
                </button>
                <button
                  type="button"
                  aria-label={`Delete ${title}`}
                  className="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 hover:bg-sidebar-accent hover:text-foreground group-hover/older:opacity-100 group-focus-within/older:opacity-100"
                  onClick={() => setDeleteId(chat.id)}
                >
                  <Trash2 size={12} />
                </button>
              </li>
            )
          })}
        </ul>
      )}
      <ConfirmDialog
        isOpen={deleteTarget !== null}
        title="Delete older chat"
        message={
          deleteTarget
            ? `Delete “${deleteTarget.title || UNTITLED_HOST_TITLE}”? This removes the chat record. Files in the original folder are kept.`
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

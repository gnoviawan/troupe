import { useCallback, useRef, useState } from 'react'
import { ChatEntryIcon } from '@/components/chat/ChatHistoryEntryRow'
import { ProjectIcon } from '@/components/ProjectIcon'
import { ChatRowTrail, type ProjectChatEntry } from '@/components/sidebar/project-chat-row'
import { usePaneDnd } from '@/hooks/use-pane-dnd'
import { cn } from '@/lib/utils'
import type { Project } from '@/types/project'
import type { ChatCardData } from '../chat-cards'
import { RAIL_HOVER_PANEL_PADDING, RAIL_PROJECT_ROW_HEIGHT, RAIL_ROW_GAP } from '../rail-layout'
import { useChatCardActions } from '../use-chat-card-actions'
import { isOpenChatCard, useChatLiveState, useProjectChats } from '../use-project-chats'
import {
  RailChatGroup,
  RailChatIcon,
  RailChatItem,
  RailChatList,
  RailChatRow,
  RailChatStatusDot,
  RailChatTitle,
  RailShowMore
} from './rail-chats'
import { RailDensityProvider, useRailCollapsed } from './rail-density'
import {
  RailHoverPanel,
  RailHoverPanelContent,
  RailHoverPanelProjectRow,
  RailHoverPanelTrigger
} from './rail-hover-panel'
import { logRailEvent } from './rail-log'
import { RAIL_WORKTREE_CHIP_HEIGHT, RailWorktreeChip, usePickedWorktree } from './worktree-switcher'

/**
 * The chat peek opens 4px above the project row that sits on top of the
 * chats, so its project row and chat rows line up with the Rail icons.
 */
const PEEK_ALIGN_OFFSET = -(RAIL_PROJECT_ROW_HEIGHT + RAIL_ROW_GAP + RAIL_HOVER_PANEL_PADDING)

/** A click this soon after a drag ends is the tail of the drag, not an open. */
const CLICK_AFTER_DRAG_MS = 250

interface RailProjectChatRowProps extends React.ComponentPropsWithoutRef<'button'> {
  card: ChatCardData
  open: boolean
  dragging: boolean
  onOpenChat: (entry: ProjectChatEntry) => void
  onDragChat: (entry: ProjectChatEntry, event: React.DragEvent) => void
  onDragChatEnd: () => void
}

/**
 * One chat of the project: agent icon, title and the live trail. On the
 * collapsed Rail the trail gives way to a status dot on the icon; the status
 * stays in the accessible name. Drag the row onto a pane edge to split, or
 * into a pane to open the chat there.
 */
function RailProjectChatRow({
  card,
  open,
  dragging,
  onOpenChat,
  onDragChat,
  onDragChatEnd,
  ...props
}: RailProjectChatRowProps): React.JSX.Element {
  const { entry } = card
  const collapsed = useRailCollapsed()
  const { liveState, statusWord } = useChatLiveState(entry.id)
  const dragEndedAt = useRef<number | null>(null)
  return (
    <RailChatRow
      open={open}
      dragging={dragging}
      draggable
      onDragStart={(event) => {
        dragEndedAt.current = null
        onDragChat(entry, event)
      }}
      onDragEnd={() => {
        dragEndedAt.current = performance.now()
        onDragChatEnd()
      }}
      onClick={() => {
        const endedAt = dragEndedAt.current
        dragEndedAt.current = null
        if (endedAt !== null && performance.now() - endedAt < CLICK_AFTER_DRAG_MS) return
        onOpenChat(entry)
      }}
      title={collapsed ? undefined : entry.title}
      aria-label={statusWord ? `${entry.title}, ${statusWord}` : entry.title}
      {...props}
    >
      <RailChatIcon>
        <ChatEntryIcon
          agentId={entry.agentId}
          agentConfigId={entry.agentConfigId}
          agents={entry.agents}
        />
        {collapsed && <RailChatStatusDot state={liveState} />}
      </RailChatIcon>
      <RailChatTitle>{entry.title}</RailChatTitle>
      {!collapsed && (
        <ChatRowTrail
          liveState={liveState}
          lastActivityAt={entry.lastActivityAt}
          closed={entry.status === 'closed'}
        />
      )}
    </RailChatRow>
  )
}

interface RailProjectChatsProps {
  project: Project
  /** Inside the chat peek: rows take the 6% hover fill of the hover panel. */
  peek?: boolean
  /** Peek only: the chat whose Rail icon the pointer is on (hover fill). */
  highlightedChatId?: string | null
  /** Runs after a row opens its chat. */
  onChatOpened?: (entry: ProjectChatEntry) => void
  /** Runs when a row starts a drag into a pane. */
  onChatDragStart?: () => void
  /** Runs when that drag ends (dropped or cancelled). */
  onChatDragEnd?: () => void
}

/**
 * The active project's chats in the Rail (Sidebar V2): the chat cards scoped
 * by the worktree pick, newest first; the first five, then
 * "Show N more". A click opens or focuses the chat; a drag onto a pane edge
 * splits the pane, a drag into a pane opens the chat there.
 *
 * Collapsed Rail: one agent icon per chat at the same y, "···" for "Show N
 * more", and a hover peek next to the Rail with the project row and these
 * rows drawn expanded.
 */
export function RailProjectChats({
  project,
  peek = false,
  highlightedChatId = null,
  onChatOpened,
  onChatDragStart,
  onChatDragEnd
}: RailProjectChatsProps): React.JSX.Element | null {
  const collapsed = useRailCollapsed()
  const { cards, openChatId } = useProjectChats(project)
  const { onOpen } = useChatCardActions()
  const { startChatDrag, dragPayload } = usePaneDnd()
  const [hoveredChatId, setHoveredChatId] = useState<string | null>(null)
  const [peekOpen, setPeekOpen] = useState(false)
  // A drag from the peek keeps the peek mounted (hidden) until the drag
  // ends: an unmounted drag source gets no dragend, so the drop zones
  // would stay up after a cancelled drag.
  const [peekDragging, setPeekDragging] = useState(false)
  const draggedChatId = dragPayload?.type === 'chat' ? dragPayload.sessionId : null

  const openChat = useCallback(
    (entry: ProjectChatEntry) => {
      logRailEvent(`${peek ? 'peek ' : ''}open chat ${entry.id}`)
      onOpen(entry)
      onChatOpened?.(entry)
    },
    [onOpen, onChatOpened, peek]
  )
  const closePeek = useCallback(() => setPeekOpen(false), [])
  // Close on the next frame: the browser takes the drag image after
  // dragstart, so the source must still be visible then.
  const closePeekAfterDragStart = useCallback(() => {
    requestAnimationFrame(() => setPeekOpen(false))
  }, [])
  const startPeekDrag = useCallback(() => {
    setPeekDragging(true)
    closePeekAfterDragStart()
  }, [closePeekAfterDragStart])
  const endPeekDrag = useCallback(() => setPeekDragging(false), [])
  const dragChat = useCallback(
    (entry: ProjectChatEntry, event: React.DragEvent) => {
      logRailEvent(`${peek ? 'peek ' : ''}drag chat ${entry.id}`)
      startChatDrag(entry.id, event)
      if (collapsed) closePeekAfterDragStart()
      onChatDragStart?.()
    },
    [startChatDrag, peek, collapsed, closePeekAfterDragStart, onChatDragStart]
  )
  const endDragChat = useCallback(() => onChatDragEnd?.(), [onChatDragEnd])
  // The peek repeats the worktree chip, so it moves up by the chip line too.
  const chipOffset = usePickedWorktree(project) ? RAIL_WORKTREE_CHIP_HEIGHT : 0

  if (cards.length === 0) return null
  const openIndex = cards.findIndex((card) => isOpenChatCard(card, openChatId))
  const group = (
    <RailChatGroup
      total={cards.length}
      pinnedIndex={openIndex === -1 ? undefined : openIndex}
      data-testid={peek ? 'rail-chat-peek-chats' : 'rail-project-chats'}
    >
      <RailChatList aria-label={`Chats in ${project.name}`}>
        {cards.map((card, index) => {
          const id = card.entry.id
          return (
            <RailChatItem key={id} index={index}>
              <RailProjectChatRow
                card={card}
                open={isOpenChatCard(card, openChatId)}
                dragging={id === draggedChatId}
                onOpenChat={openChat}
                onDragChat={dragChat}
                onDragChatEnd={endDragChat}
                data-highlighted={(peek && id === highlightedChatId) || undefined}
                className={cn(
                  peek && 'hover:bg-foreground/[0.06] data-[highlighted]:bg-foreground/[0.06]'
                )}
                onPointerEnter={collapsed ? () => setHoveredChatId(id) : undefined}
                onPointerLeave={
                  collapsed
                    ? () => setHoveredChatId((current) => (current === id ? null : current))
                    : undefined
                }
              />
            </RailChatItem>
          )
        })}
      </RailChatList>
      <RailShowMore />
    </RailChatGroup>
  )
  if (!collapsed) return group

  return (
    <RailHoverPanel open={peekOpen} onOpenChange={setPeekOpen}>
      <RailHoverPanelTrigger asChild>{group}</RailHoverPanelTrigger>
      <RailHoverPanelContent
        alignOffset={PEEK_ALIGN_OFFSET - chipOffset}
        forceMount={peekDragging || undefined}
        className="data-[state=closed]:invisible"
        data-testid="rail-chat-peek"
      >
        <RailDensityProvider density="expanded">
          <RailHoverPanelProjectRow
            icon={<ProjectIcon project={project} size={16} />}
            name={project.name}
          />
          <RailWorktreeChip project={project} collapsed={false} />
          <RailProjectChats
            project={project}
            peek
            highlightedChatId={hoveredChatId}
            onChatOpened={closePeek}
            onChatDragStart={startPeekDrag}
            onChatDragEnd={endPeekDrag}
          />
        </RailDensityProvider>
      </RailHoverPanelContent>
    </RailHoverPanel>
  )
}

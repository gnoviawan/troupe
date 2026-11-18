import { openNewChat } from '@/components/chat/new-chat/open-new-chat'
import { ChatEdit, Globe, Plus, Terminal } from '@/components/icons'
import { logShellEvent } from '@/components/shell/shell-log'
import { useAvailableShells } from '@/components/shell/tool-strip/use-available-shells'
import { useShortcutLabel } from '@/components/shell/use-shortcut-label'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { isTauriContext } from '@/lib/tauri-runtime'
import { cn } from '@/lib/utils'
import { useDefaultShell } from '@/stores/app-settings-store'
import { useActiveProject } from '@/stores/project-store'
import { useNewTabActions } from './new-tab-actions'

/** Menu item icons: 14px with the menu's icon gap (docs/design/menus.md). */
const ICON = 'mr-2 size-3.5'

interface NewTabMenuProps {
  paneId: string
}

/**
 * The tab bar `+`: one menu for a new chat, a new terminal (the default
 * shell, or another detected shell from the submenu) and a new browser tab
 * (desktop only), each opening in this pane. Radix owns keyboard, Escape and
 * focus return.
 */
export function NewTabMenu({ paneId }: NewTabMenuProps): React.JSX.Element {
  const actions = useNewTabActions()
  const project = useActiveProject()
  const appShell = useDefaultShell()
  const { loading, sortedShells, defaultShell } = useAvailableShells(
    project?.defaultShell || appShell
  )
  const chatShortcut = useShortcutLabel('newTerminal')
  const terminalShortcut = useShortcutLabel('worktreeOpenTerminal')
  const browserShortcut = useShortcutLabel('newBrowserTab')
  const canBrowse = isTauriContext()

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className={cn(
            'flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 ease-out hover:bg-foreground/[0.06] hover:text-foreground data-[state=open]:bg-foreground/[0.06] data-[state=open]:text-foreground',
            FOCUS_RING_CLASS
          )}
          title="New tab"
          aria-label="New tab"
          data-testid="new-tab-menu-trigger"
        >
          <Plus size={14} aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuItem
          disabled={!project?.path}
          onSelect={() => {
            logShellEvent(`new tab menu opens a chat in pane ${paneId}`)
            void openNewChat({ paneId, reason: 'new-tab-menu' })
          }}
        >
          <ChatEdit className={ICON} aria-hidden />
          New chat
          {chatShortcut && <DropdownMenuShortcut>{chatShortcut}</DropdownMenuShortcut>}
        </DropdownMenuItem>

        <DropdownMenuItem
          disabled={!actions}
          onSelect={() => {
            logShellEvent(`new tab menu opens a terminal in pane ${paneId}`)
            actions?.addTerminal(paneId)
          }}
        >
          <Terminal className={ICON} aria-hidden />
          New terminal
          {terminalShortcut && <DropdownMenuShortcut>{terminalShortcut}</DropdownMenuShortcut>}
        </DropdownMenuItem>

        <DropdownMenuSub>
          <DropdownMenuSubTrigger disabled={!actions}>
            <Terminal className={ICON} aria-hidden />
            New terminal with
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="w-52" aria-label="Shells">
            {loading ? (
              <DropdownMenuItem disabled>Detecting shells…</DropdownMenuItem>
            ) : sortedShells && sortedShells.length > 0 ? (
              sortedShells.map((shell) => (
                <DropdownMenuItem
                  key={shell.path}
                  onSelect={() => {
                    logShellEvent(
                      `new tab menu opens a terminal with shell ${shell.name} in pane ${paneId}`
                    )
                    actions?.addTerminal(paneId, shell)
                  }}
                >
                  <span className="min-w-0 flex-1 truncate">{shell.displayName}</span>
                  {shell === defaultShell && (
                    <span className="ml-2 text-2xs text-muted-foreground">Default</span>
                  )}
                </DropdownMenuItem>
              ))
            ) : (
              <DropdownMenuItem disabled>No shells detected</DropdownMenuItem>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>

        {canBrowse && (
          <DropdownMenuItem
            disabled={!actions}
            onSelect={() => {
              logShellEvent(`new tab menu opens a browser tab in pane ${paneId}`)
              actions?.addBrowserTab(paneId)
            }}
          >
            <Globe className={ICON} aria-hidden />
            New browser tab
            {browserShortcut && <DropdownMenuShortcut>{browserShortcut}</DropdownMenuShortcut>}
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

import { Check } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { cn } from '@/lib/utils'
import { AccountAvatar, DefaultPill } from './ClaudeAccountRow'

/*
 * One option of a Project settings account picker (Codex and Antigravity):
 * a listbox row with arrow-key focus, and a Sign in button beside a
 * signed-out Account.
 */

function focusOption(current: HTMLElement, direction: 1 | -1): void {
  const list = current.closest('[role="listbox"]')
  const options = [...(list?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])]
  const index = options.indexOf(current)
  if (index < 0 || options.length === 0) return
  options[(index + direction + options.length) % options.length]?.focus()
}

export function AccountOptionRow({
  label,
  sub,
  selected,
  disabled,
  badge,
  tabStop,
  onSelect,
  onSignIn
}: {
  label: string
  sub: string
  selected: boolean
  disabled: boolean
  badge?: boolean
  tabStop: boolean
  onSelect: () => void
  onSignIn?: () => void
}): React.JSX.Element {
  return (
    <div className="flex items-center gap-2">
      <div
        role="option"
        aria-selected={selected}
        aria-disabled={disabled || undefined}
        tabIndex={tabStop ? 0 : -1}
        onClick={disabled ? undefined : onSelect}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault()
            focusOption(event.currentTarget, event.key === 'ArrowDown' ? 1 : -1)
            return
          }
          if (disabled) return
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            onSelect()
          }
        }}
        className={cn(
          'flex min-w-0 flex-1 items-center gap-3 rounded-lg px-3 py-2.5 outline-hidden',
          FOCUS_RING_CLASS,
          selected && 'bg-foreground/[0.03] ring-1 ring-inset ring-foreground/[0.12]',
          !disabled && !selected && 'hover:bg-foreground/[0.02]',
          disabled && 'opacity-60'
        )}
      >
        <AccountAvatar name={label} />
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex min-w-0 items-center gap-1.5 text-xs font-medium text-foreground">
            <span className="truncate">{label}</span>
            {badge && <DefaultPill accountName={label} />}
          </span>
          <span className="truncate text-2xs text-muted-foreground">{sub}</span>
        </span>
        <span className="flex size-4 items-center justify-center text-foreground">
          {selected && <Check size={15} />}
        </span>
      </div>
      {onSignIn && (
        <Button type="button" size="xs" variant="outline" onClick={onSignIn}>
          Sign in
        </Button>
      )}
    </div>
  )
}

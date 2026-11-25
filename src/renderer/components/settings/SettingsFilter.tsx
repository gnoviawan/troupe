import { Search, X } from '@/components/icons'
import { FOCUS_RING_CLASS, QUIET_ICON_BUTTON_CLASS } from '@/components/ui/panel-styles'
import { cn } from '@/lib/utils'

interface SettingsFilterProps {
  value: string
  onValueChange: (value: string) => void
  /** Placeholder and accessible name, e.g. "Filter shortcuts". */
  label: string
  placeholder?: string
  className?: string
}

/**
 * The 32px filter well used above a settings list (agents, skills, MCP
 * servers, shortcuts, tools): a sunk field with a search glyph and a clear
 * button once it has text.
 */
export function SettingsFilter({
  value,
  onValueChange,
  label,
  placeholder,
  className
}: SettingsFilterProps): React.JSX.Element {
  return (
    <div className={cn('relative min-w-0', className)}>
      <Search
        size={14}
        aria-hidden
        className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
      />
      <input
        type="text"
        value={value}
        onChange={(event) => onValueChange(event.target.value)}
        placeholder={placeholder ?? label}
        aria-label={label}
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        enterKeyHint="search"
        className={cn(
          'h-8 w-full rounded-md border-0 bg-muted pl-8 pr-8 text-xs font-medium text-foreground placeholder:text-muted-foreground',
          'pointer-coarse:h-11 pointer-coarse:pr-11 pointer-coarse:text-base',
          FOCUS_RING_CLASS
        )}
      />
      {value && (
        <button
          type="button"
          onClick={() => onValueChange('')}
          aria-label="Clear filter"
          className={cn(
            QUIET_ICON_BUTTON_CLASS,
            'absolute right-1 top-1/2 flex size-6 -translate-y-1/2 pointer-coarse:right-0 pointer-coarse:size-11'
          )}
        >
          <X size={13} />
        </button>
      )}
    </div>
  )
}

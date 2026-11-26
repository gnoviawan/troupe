import { Terminal } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import type { ConsoleEntry, ConsoleLevel } from './console-bridge'

const LEVEL_INK: Readonly<Record<ConsoleLevel, string>> = {
  log: 'text-muted-foreground',
  info: 'text-muted-foreground',
  warn: 'text-warning',
  error: 'text-destructive'
}

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? '' : 's'}`
}

/** The console count in the preview bar; errors, when there are any, replace the total. */
export function ConsoleToggle({
  entries,
  open,
  onToggle,
  touch
}: {
  entries: readonly ConsoleEntry[]
  open: boolean
  onToggle: () => void
  touch: boolean
}): React.JSX.Element {
  const errors = entries.filter((entry) => entry.level === 'error').length
  const summary = errors > 0 ? plural(errors, 'error') : plural(entries.length, 'message')
  const label = open ? 'Hide console' : 'Show console'
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size={touch ? 'touch' : 'xs'}
          aria-label={`${label}, ${summary}`}
          aria-pressed={open}
          onClick={onToggle}
          className={cn(
            'gap-1 font-normal tabular-nums [&_svg]:size-3.5',
            touch ? 'min-w-11 px-3 text-xs' : 'h-6 px-1.5',
            open ? 'bg-foreground/[0.06] text-foreground' : 'text-muted-foreground',
            errors > 0 && 'text-destructive'
          )}
        >
          <Terminal aria-hidden="true" />
          <span>{errors > 0 ? summary : entries.length}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent className="text-xs">{label}</TooltipContent>
    </Tooltip>
  )
}

export function ConsolePanel({ entries }: { entries: readonly ConsoleEntry[] }): React.JSX.Element {
  return (
    <div
      role="log"
      aria-label="Preview console"
      className="h-32 shrink-0 overflow-auto border-t-[0.5px] border-border bg-card px-3 py-1.5 font-mono text-xs"
    >
      {entries.length === 0 ? (
        <p className="text-muted-foreground">No output</p>
      ) : (
        entries.map((entry, index) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: rows are plain text with no state
          <div key={index} className="flex gap-2 whitespace-pre-wrap break-words">
            <span className={cn('shrink-0', LEVEL_INK[entry.level])}>{entry.level}</span>
            <span className="min-w-0 text-secondary-foreground">{entry.text}</span>
          </div>
        ))
      )}
    </div>
  )
}

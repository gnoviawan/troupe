import { useRef } from 'react'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { cn } from '@/lib/utils'

export interface SegmentedOption<T extends string> {
  value: T
  label: string
  /** Quiet count after the label ("All 191"). */
  count?: number
  disabled?: boolean
}

interface SettingsSegmentedProps<T extends string> {
  /** Accessible name of the whole control. */
  label: string
  options: ReadonlyArray<SegmentedOption<T>>
  value: T
  onValueChange: (value: T) => void
  /** `row`: 28px, inside a card row. `toolbar`: 32px, next to a filter field. */
  size?: 'row' | 'toolbar'
  disabled?: boolean
  /** Id of the text that describes the control (the row description). */
  describedBy?: string
  id?: string
  className?: string
}

/**
 * Pick one of a few options in place (release channel, skill scope). A sunk
 * track with the chosen option raised like a secondary button. It is a radio
 * group: one tab stop, and the arrow keys move the choice.
 */
export function SettingsSegmented<T extends string>({
  label,
  options,
  value,
  onValueChange,
  size = 'row',
  disabled,
  describedBy,
  id,
  className
}: SettingsSegmentedProps<T>): React.JSX.Element {
  const trackRef = useRef<HTMLDivElement>(null)
  const enabled = options.filter((option) => !option.disabled)

  const move = (step: 1 | -1): void => {
    const index = enabled.findIndex((option) => option.value === value)
    const next = enabled[(index + step + enabled.length) % enabled.length]
    if (!next) return
    onValueChange(next.value)
    trackRef.current
      ?.querySelector<HTMLButtonElement>(`[data-value="${CSS.escape(next.value)}"]`)
      ?.focus()
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      event.preventDefault()
      move(1)
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      event.preventDefault()
      move(-1)
    }
  }

  return (
    <div
      ref={trackRef}
      id={id}
      role="radiogroup"
      aria-label={label}
      aria-describedby={describedBy}
      aria-disabled={disabled || undefined}
      onKeyDown={disabled ? undefined : handleKeyDown}
      className={cn(
        'inline-flex shrink-0 items-center gap-0.5 rounded-md bg-background p-0.5 shadow-[inset_0_0_0_0.5px_oklch(var(--border))]',
        size === 'toolbar' ? 'h-8' : 'h-7',
        'pointer-coarse:h-11',
        disabled && 'opacity-60',
        className
      )}
    >
      {options.map((option) => {
        const chosen = option.value === value
        const optionDisabled = disabled || option.disabled
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={chosen}
            data-value={option.value}
            aria-label={option.count !== undefined ? `${option.label} ${option.count}` : undefined}
            tabIndex={chosen ? 0 : -1}
            disabled={optionDisabled}
            onClick={() => onValueChange(option.value)}
            className={cn(
              'flex h-full items-center gap-1.5 rounded-[4px] px-2.5 text-xs font-medium transition-colors duration-150 ease-out',
              FOCUS_RING_CLASS,
              chosen
                ? 'bg-muted text-foreground shadow-control'
                : 'text-muted-foreground hover:text-foreground disabled:hover:text-muted-foreground'
            )}
          >
            {option.label}
            {option.count !== undefined && (
              <span className="tabular-nums text-muted-foreground">{option.count}</span>
            )}
          </button>
        )
      })}
    </div>
  )
}

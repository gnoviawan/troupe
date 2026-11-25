import { cn } from '@/lib/utils'

interface ProgressBarProps {
  /** Accessible name, e.g. "Download progress". */
  label: string
  /** 0–100. Leave out for work with no known end (a short sliver moves). */
  value?: number
  /** Read aloud instead of the number, e.g. "42% complete". */
  valueText?: string
  /** Fill colour class. Default: the primary fill. */
  fillClassName?: string
  /** A small value still shows a sliver so the bar never looks empty. */
  minVisible?: number
  className?: string
}

/**
 * A 4px progress track. With a `value` the fill grows to it; without one the
 * bar is indeterminate and a sliver moves across (still under reduced motion).
 */
export function ProgressBar({
  label,
  value,
  valueText,
  fillClassName = 'bg-primary-fill',
  minVisible = 0,
  className
}: ProgressBarProps): React.JSX.Element {
  const determinate = value !== undefined
  const percent = determinate ? Math.max(0, Math.min(100, value)) : 0
  const width = determinate && percent > 0 ? Math.max(minVisible, percent) : 0
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={determinate ? 0 : undefined}
      aria-valuemax={determinate ? 100 : undefined}
      aria-valuenow={determinate ? Math.round(percent) : undefined}
      aria-valuetext={valueText}
      className={cn('relative h-1 w-full overflow-hidden rounded-full bg-muted', className)}
    >
      {determinate ? (
        <div
          className={cn(
            'h-full rounded-full transition-[width] duration-300 ease-out motion-reduce:transition-none',
            fillClassName
          )}
          style={{ width: `${width}%` }}
        />
      ) : (
        <div
          className={cn(
            'absolute inset-y-0 left-0 w-1/5 rounded-full animate-progress-sliver motion-reduce:animate-none motion-reduce:left-[40%]',
            fillClassName
          )}
        />
      )}
    </div>
  )
}

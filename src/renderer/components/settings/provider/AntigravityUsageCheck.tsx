import type { AntigravityAccountUsage } from '@shared/types/antigravity-accounts.types'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { formatAntigravityChecked } from '@/lib/antigravity-usage-format'

interface AntigravityUsageCheckProps {
  accountName: string
  read: AntigravityAccountUsage | undefined
  checking: boolean
  canCheck: boolean
  onCheck: () => void
}

export function AntigravityUsageCheck({
  accountName,
  read,
  checking,
  canCheck,
  onCheck
}: AntigravityUsageCheckProps): React.JSX.Element | null {
  const checked = read && read.status !== 'not_checked' ? read : null
  if (!canCheck && !checked) return null
  return (
    <div className="mt-1 flex flex-col gap-1.5">
      {checked?.message && (
        <p className="whitespace-pre-line text-2xs text-muted-foreground">{checked.message}</p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {canCheck && (
          <Button
            type="button"
            size="xs"
            variant="outline"
            disabled={checking}
            onClick={onCheck}
            aria-label={`Check usage of ${accountName}`}
            className="pointer-coarse:h-11 pointer-coarse:px-3"
          >
            Check usage
          </Button>
        )}
        <span
          className="flex items-center gap-1.5 text-2xs text-muted-foreground"
          aria-live="polite"
        >
          {checking ? (
            <>
              <Spinner size={11} decorative />
              Reading usage…
            </>
          ) : checked ? (
            formatAntigravityChecked(checked.checkedAt)
          ) : null}
        </span>
      </div>
    </div>
  )
}

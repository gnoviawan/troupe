import { useState } from 'react'
import { toast } from 'sonner'
import { Check, ExternalLink, Pin } from '@/components/icons'
import { toastAccountChoiceError } from '@/components/settings/provider/account-choice-error'
import { StatusDot } from '@/components/settings/provider/account-status'
import {
  ANTIGRAVITY_NOT_READY_REASON,
  antigravityAccountReadiness
} from '@/components/settings/provider/antigravity-account-status'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { Spinner } from '@/components/ui/spinner'
import {
  antigravityChipUsage,
  antigravityWindowLabel,
  planLabel,
  primaryAntigravityWindow,
  type UsageReading,
  type UsageTone,
  usageTone
} from '@/lib/antigravity-usage-format'
import { cn } from '@/lib/utils'
import {
  chatSwitchToast,
  showChatSwitchToast,
  switchChatAntigravityAccount,
  switchProjectAntigravityAccount
} from '@/stores/antigravity-account-switch'
import { useSettingsModalStore } from '@/stores/settings-modal-store'
import {
  type ChatAntigravityAccount,
  useChatAntigravityAccount
} from './use-chat-antigravity-account'
import {
  type ProjectAntigravityAccount,
  useProjectAntigravityAccount
} from './use-project-antigravity-account'

const TONE_TEXT: Record<UsageTone, string> = {
  normal: 'text-secondary-foreground',
  warning: 'text-warning',
  critical: 'text-destructive'
}

const DOT_TONE = { normal: 'ok', warning: 'warning', critical: 'critical' } as const

/** A model name keeps its case: "Gemini 3 Pro 40% used". */
const spoken = (reading: UsageReading): string => `${reading.label} ${reading.percent}% used`

/** Composer chip for an Antigravity chat: its Account and its most used model. */
export function AntigravityAccountChip({
  sessionId
}: {
  sessionId: string
}): React.JSX.Element | null {
  const ctx = useChatAntigravityAccount(sessionId)
  return <AccountChip ctx={ctx} chat={ctx} />
}

/** The same chip before a chat exists (the agent launcher). */
export function ProjectAntigravityAccountChip({
  projectId
}: {
  projectId: string
}): React.JSX.Element | null {
  return <AccountChip ctx={useProjectAntigravityAccount(projectId)} chat={null} />
}

function AccountChip({
  ctx,
  chat
}: {
  ctx: ProjectAntigravityAccount | null
  chat: ChatAntigravityAccount | null
}): React.JSX.Element | null {
  const openAppSettings = useSettingsModalStore((s) => s.openApp)
  const [switching, setSwitching] = useState(false)
  if (!ctx) return null

  const readiness = antigravityAccountReadiness(ctx.current.account, ctx.current.usage)
  const signedOut = readiness !== 'ready'
  const used = signedOut ? null : antigravityChipUsage(ctx.current.usage)
  const segments = used ? [used.main, ...(used.other ? [used.other] : [])] : []
  const tone = usageTone(Math.max(0, ...segments.map((segment) => segment.percent)))
  const snapshot = ctx.accounts.snapshot

  const choose = async (accountId: string | null): Promise<void> => {
    const nextId = accountId ?? (chat ? chat.projectAccount.id : snapshot?.defaultAccountId) ?? null
    const next = snapshot?.accounts.find((a) => a.id === nextId)
    if (chat) {
      const ownId = chat.ownAccount ? chat.current.account.id : null
      if (accountId === ownId) return
    } else if (accountId === (ctx.explicit ? ctx.current.account.id : null)) {
      return
    }
    setSwitching(true)
    try {
      if (chat) {
        const moved = await switchChatAntigravityAccount(
          {
            sessionId: chat.sessionId,
            projectId: chat.projectId,
            runsOn: chat.current.account.id,
            ownAccountId: chat.ownAccount ? chat.current.account.id : null,
            projectAccountId: chat.projectAccount.id
          },
          accountId
        )
        await ctx.accounts.reload()
        showChatSwitchToast(
          chatSwitchToast(moved, accountId === null ? null : (next?.name ?? null))
        )
      } else {
        await switchProjectAntigravityAccount(ctx.projectId, accountId)
        await ctx.accounts.reload()
        toast.success(next ? `This project now uses ${next.name}` : 'Account changed')
      }
    } catch (err) {
      toastAccountChoiceError(err, 'Could not switch the account')
      void ctx.accounts.reload()
    } finally {
      setSwitching(false)
    }
  }

  const name = ctx.current.account.name
  const followAccount = chat ? chat.projectAccount : ctx.defaultAccount
  const followReadiness = followAccount
    ? antigravityAccountReadiness(followAccount, ctx.accounts.usage[followAccount.id])
    : 'ready'

  return (
    <DropdownMenu
      onOpenChange={(open) => {
        if (!open) return
        void ctx.accounts.reload()
        void ctx.accounts.refreshUsage(false)
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          disabled={switching}
          aria-busy={switching}
          data-testid="antigravity-account-chip"
          className={cn(
            'relative flex h-8 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-2xs text-muted-foreground transition-colors ease-out',
            "after:absolute after:-inset-1 after:content-['']",
            'hover:text-foreground data-[state=open]:text-foreground disabled:opacity-60'
          )}
          aria-label={`Antigravity account: ${[name, ...segments.map(spoken)].join(', ')}. Change account`}
        >
          {switching ? (
            <Spinner size={10} decorative />
          ) : (
            <StatusDot tone={signedOut ? 'signed-out' : DOT_TONE[tone]} />
          )}
          <span className="max-w-24 truncate">{name}</span>
          {chat?.ownAccount && <Pin size={10} aria-label="Account for this chat only" />}
          {segments.map((segment, index) => (
            <span key={segment.short} className="flex items-center gap-1.5 tabular-nums">
              {index > 0 && <span aria-hidden>·</span>}
              <span className={cn('flex min-w-0 items-center gap-1', TONE_TEXT[segment.tone])}>
                <span className="max-w-24 truncate">{segment.short}</span>
                {segment.percent}%
              </span>
            </span>
          ))}
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        <DropdownMenuLabel>
          {chat ? 'Account for this chat' : 'Account for this project'}
        </DropdownMenuLabel>
        {followAccount && (
          <DropdownMenuItem
            disabled={followReadiness !== 'ready'}
            onSelect={() => void choose(null)}
            className="flex items-center gap-2"
          >
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-xs text-foreground">
                {chat ? 'Follow project' : 'Follow default'}
              </span>
              <span className="truncate text-2xs text-muted-foreground">
                Uses {followAccount.name}
                {followReadiness === 'ready'
                  ? ''
                  : ` · ${ANTIGRAVITY_NOT_READY_REASON[followReadiness].toLowerCase()}`}
              </span>
            </span>
            <span className="flex size-4 shrink-0 items-center justify-center">
              {(chat ? !chat.ownAccount : !ctx.explicit) && <Check size={14} />}
            </span>
          </DropdownMenuItem>
        )}
        {(snapshot?.accounts ?? []).map((account) => {
          const rowUsage = ctx.accounts.usage[account.id]
          const rowReady = antigravityAccountReadiness(account, rowUsage)
          const top = primaryAntigravityWindow(rowUsage)
          const detail =
            rowReady === 'ready'
              ? [
                  planLabel(rowUsage?.planType ?? null),
                  top ? `${antigravityWindowLabel(top)} ${Math.round(top.percent)}%` : null
                ]
                  .filter(Boolean)
                  .join(' · ') || 'Signed in'
              : ANTIGRAVITY_NOT_READY_REASON[rowReady]
          return (
            <DropdownMenuItem
              key={account.id}
              disabled={rowReady !== 'ready'}
              onSelect={() => void choose(account.id)}
              className="flex items-center gap-2"
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-xs text-foreground">{account.name}</span>
                <span className="truncate text-2xs text-muted-foreground">{detail}</span>
              </span>
              <span className="flex size-4 shrink-0 items-center justify-center">
                {chat
                  ? chat.ownAccount && account.id === ctx.current.account.id && <Check size={14} />
                  : ctx.explicit && account.id === ctx.current.account.id && <Check size={14} />}
              </span>
            </DropdownMenuItem>
          )
        })}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={() => openAppSettings('provider')}
          className="flex items-center gap-2"
        >
          <span className="flex-1 text-xs">Manage accounts</span>
          <ExternalLink size={12} className="text-muted-foreground" />
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

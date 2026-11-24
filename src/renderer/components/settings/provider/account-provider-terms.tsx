import { ConfirmDialog } from '@/components/ConfirmDialog'
import { TOUCH_HIT_AREA_CLASS } from '@/components/phone-modal-classes'
import { cn } from '@/lib/utils'

export type AccountProviderId = 'claude' | 'codex' | 'antigravity'

interface ProviderTerms {
  name: string
  /** The company whose terms apply to the provider's plans. */
  company: string
  /** Why switching accounts at a limit can break this provider's terms. */
  risk: string
  /** The risks section of the provider's accounts page in the docs. */
  docsUrl: string
}

const docsUrl = (page: string): string =>
  `https://usetroupe.dev/docs/agent-setup/${page}#risks-of-switching-accounts`

export const PROVIDER_TERMS: Record<AccountProviderId, ProviderTerms> = {
  claude: {
    name: 'Claude',
    company: 'Anthropic',
    risk: 'Anthropic says Claude plan limits assume ordinary, individual use. Its terms do not allow access by automated means unless Anthropic permits it, and Anthropic can suspend an account without notice.',
    docsUrl: docsUrl('claude-accounts')
  },
  codex: {
    name: 'Codex',
    company: 'OpenAI',
    risk: "OpenAI's terms do not allow getting around rate limits or restrictions. Moving a chat to another account at its Codex limit can count as that. OpenAI's own answer to a Codex limit is to buy credits or use an API key.",
    docsUrl: docsUrl('codex-accounts')
  },
  antigravity: {
    name: 'Antigravity',
    company: 'Google',
    risk: "Google's Antigravity FAQ says automated account rotation is not supported on consumer plans. Google has suspended Antigravity access for breaking its terms.",
    docsUrl: docsUrl('antigravity-accounts')
  }
}

const SHARED_SWITCH_RISK =
  'A provider can suspend all the accounts of one person together. Safer options are to switch by hand when a chat reaches its limit, buy extra usage or credits, or use an API key.'

function DocsLink({ href, children }: { href: string; children: string }): React.JSX.Element {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className={cn('text-foreground underline underline-offset-2', TOUCH_HIT_AREA_CLASS)}
    >
      {children}
    </a>
  )
}

/** The warning a user accepts before automatic account switching turns on. */
export function AutoSwitchRiskDialog({
  provider,
  open,
  onConfirm,
  onCancel
}: {
  provider: AccountProviderId
  open: boolean
  onConfirm: () => void
  onCancel: () => void
}): React.JSX.Element {
  const terms = PROVIDER_TERMS[provider]
  return (
    <ConfirmDialog
      isOpen={open}
      title={`Switch ${terms.name} accounts automatically?`}
      message={terms.risk}
      confirmLabel="Turn on anyway"
      variant="danger"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <p className="text-sm text-muted-foreground">
        {SHARED_SWITCH_RISK} <DocsLink href={terms.docsUrl}>Read more</DocsLink>
      </p>
    </ConfirmDialog>
  )
}

import { toast } from 'sonner'
import { accountSignedOutMessage, isAccountSignedOutError } from '@/lib/agents/acp-spawn-errors'
import { useSettingsModalStore } from '@/stores/settings-modal-store'

/**
 * Tell the user why the host refused an Account choice. A signed-out Account
 * gets the host's sentence and a button to the Accounts page, where the user
 * signs in; nothing changed on the host. Any other failure keeps `fallback`.
 */
export function toastAccountChoiceError(error: unknown, fallback: string): void {
  if (isAccountSignedOutError(error)) {
    toast.error(accountSignedOutMessage(error), {
      action: {
        label: 'Open Accounts',
        onClick: () => useSettingsModalStore.getState().openApp('provider')
      }
    })
    return
  }
  toast.error(`${fallback}: ${error instanceof Error ? error.message : String(error)}`)
}

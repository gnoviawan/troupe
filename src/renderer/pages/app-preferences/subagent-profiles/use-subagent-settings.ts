import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'
import type { SubagentSettingsInput, SubagentSettingsView } from '@/lib/acp-api'
import { acpApi } from '@/lib/api'
import { logFrontendError } from '@/lib/log-api'
import { applySettingsInput } from './profile-model'

/**
 * One change to the sub-agent settings: the input that makes it on `view`,
 * or null when it no longer applies (its profile is gone).
 */
export type SettingsChange = (view: SubagentSettingsView) => SubagentSettingsInput | null

interface State {
  /** The host's last answer. */
  host: SubagentSettingsView | null
  /** Changes not answered yet, oldest first; the first is in flight. */
  queued: ReadonlyArray<{ change: SettingsChange }>
  failed: boolean
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * The sub-agent settings, with every queued change shown at once. Changes go
 * to the host one at a time, each built on the host's answer to the one
 * before, so a reply never lands on top of a newer edit and no edit sends a
 * profile an earlier one changed. A refused change toasts and reads the
 * host's view back.
 */
export function useSubagentSettings(): {
  /** The host's view with the queued changes applied; null until read. */
  view: SubagentSettingsView | null
  failed: boolean
  /** Settles once the host answers this change; rejects when it refuses it. */
  change: (change: SettingsChange) => Promise<void>
} {
  const [state, setState] = useState<State>({ host: null, queued: [], failed: false })
  const current = useRef(state)
  const put = useCallback((patch: Partial<State>) => {
    current.current = { ...current.current, ...patch }
    setState(current.current)
  }, [])
  const tail = useRef<Promise<unknown>>(Promise.resolve())
  const serially = useCallback(<T>(job: () => Promise<T>): Promise<T> => {
    const run = tail.current.then(job)
    tail.current = run.catch(() => {})
    return run
  }, [])

  const read = useCallback(async (): Promise<void> => {
    try {
      put({ host: await acpApi.getSubagentSettings(), failed: false })
    } catch (error) {
      put({ host: null, failed: true })
      void logFrontendError({
        level: 'warn',
        source: 'SubagentSettingsGroup',
        message: `Failed to read the sub-agent settings: ${errorText(error)}`
      })
    }
  }, [put])

  useEffect(() => {
    void serially(read)
  }, [serially, read])

  const change = useCallback(
    (change: SettingsChange): Promise<void> => {
      const entry = { change }
      const without = (): State['queued'] => current.current.queued.filter((e) => e !== entry)
      put({ queued: [...current.current.queued, entry] })
      return serially(async () => {
        const { host } = current.current
        const input = host && change(host)
        try {
          const answer = input ? await acpApi.setSubagentSettings(input) : host
          put({ host: answer, queued: without() })
        } catch (error) {
          toast.error('Could not save the sub-agent setting.', { description: errorText(error) })
          void logFrontendError({
            level: 'warn',
            source: 'SubagentSettingsGroup',
            message: `Failed to set subagent settings: ${errorText(error)}`
          })
          put({ queued: without() })
          await read()
          throw error
        }
      })
    },
    [put, serially, read]
  )

  const view = useMemo(
    () =>
      state.host &&
      state.queued.reduce((view, { change }) => {
        const input = change(view)
        return input ? applySettingsInput(view, input) : view
      }, state.host),
    [state]
  )
  return { view, failed: state.failed, change }
}

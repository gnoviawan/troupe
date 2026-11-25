/**
 * The mocked `gitApi` of the git store's vectors. It lives apart from the
 * recorder so the `vi.mock` factory imports no store.
 */

import { AcpTransportError } from '../../../../src/renderer/lib/acp-transport/types'
import { emit } from '../record'

/**
 * How one mocked `gitApi` call answers: `{ ok }` resolves, `{ reject, code }`
 * fails (an `AcpTransportError` when `code` is set), `{ hold }` stays in flight
 * until a `releaseHeld` step answers it with the step's own reply.
 */
export type GitReply = { ok: unknown } | { reject: string; code?: string } | { hold: true }

/** The mock's queue, filled from a vector's `context.git`, and the calls held. */
export const gitMock = {
  active: false,
  replies: [] as GitReply[],
  held: [] as Array<(reply: GitReply) => void>
}

function answer(reply: GitReply): Promise<unknown> {
  if ('hold' in reply) {
    return new Promise((resolve, reject) => {
      gitMock.held.push((release) => answer(release).then(resolve, reject))
    })
  }
  if ('reject' in reply) {
    return Promise.reject(
      reply.code ? new AcpTransportError(reply.code, reply.reject) : new Error(reply.reject)
    )
  }
  return Promise.resolve(reply.ok)
}

/** A `gitApi` method: records the call, then answers the next queued reply. */
export function gitCall(op: string) {
  return (...args: unknown[]): Promise<unknown> => {
    emit({ kind: 'git', op, args })
    const reply = gitMock.replies.shift()
    if (!reply) throw new Error(`git vector ran out of replies at ${op}`)
    return answer(reply)
  }
}

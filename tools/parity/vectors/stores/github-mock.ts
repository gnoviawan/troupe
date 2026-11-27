/**
 * The mocked `githubApi` of the GitHub store's vectors. It lives apart from
 * the recorder so the `vi.mock` factory imports no store.
 */

import { emit } from '../record'

/**
 * How one mocked call answers: an `IpcResult` (`githubApi` never throws), or
 * `{ hold }`, which stays in flight until a `releaseHeld` step answers it.
 */
export type GithubReply =
  | { success: true; data: unknown }
  | { success: false; code: string; error: string }
  | { hold: true }

export const githubMock = {
  replies: [] as GithubReply[],
  held: [] as Array<(reply: GithubReply) => void>
}

function answer(reply: GithubReply): Promise<unknown> {
  if ('hold' in reply) {
    return new Promise((resolve) => {
      githubMock.held.push((release) => void answer(release).then(resolve))
    })
  }
  return Promise.resolve(reply)
}

/** A `githubApi` method: records the call, then answers the next queued reply. */
export function githubCall(op: string) {
  return (...args: unknown[]): Promise<unknown> => {
    emit({ kind: 'github', op, args })
    const reply = githubMock.replies.shift()
    if (!reply) throw new Error(`github vector ran out of replies at ${op}`)
    return answer(reply)
  }
}

export const GITHUB_OPS = [
  'getContext',
  'listItems',
  'getItem',
  'getChecks',
  'getCheckLog',
  'merge',
  'comment',
  'checkoutPr',
  'getRepoMeta',
  'createLabel',
  'editItem',
  'setItemState'
]

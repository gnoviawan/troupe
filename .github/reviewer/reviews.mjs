// The reviews this workflow posts come from the Actions token's bot user.
const BOT_LOGIN = 'github-actions[bot]'
export const REVIEWS_PER_PAGE = 100

// True when github-actions[bot] already posted a review whose body carries
// `marker`. Reviews from anyone else are ignored, so a user cannot paste the
// marker to suppress a review. `fetchPage(page)` returns one page (1-based) of
// the PR's reviews. Pages are read until one comes back short.
export async function hasBotReview(fetchPage, marker) {
  for (let page = 1; ; page++) {
    const reviews = await fetchPage(page)
    const found = reviews.some(
      (r) =>
        r.user?.login === BOT_LOGIN &&
        r.user?.type === 'Bot' &&
        typeof r.body === 'string' &&
        r.body.includes(marker)
    )
    if (found) return true
    if (reviews.length < REVIEWS_PER_PAGE) return false
  }
}

// Claude Agent SDK error codes (SDKAssistantMessageError) that mean the
// provider refused the run: a bad or revoked key, billing or a quota, a model
// it no longer offers, or the service being down.
const PROVIDER_ERRORS = new Set([
  'authentication_failed',
  'oauth_org_not_allowed',
  'account_on_hold',
  'verification_required',
  'billing_error',
  'rate_limit',
  'overloaded',
  'model_not_found',
  'server_error',
  'cloud_credential_error'
])

// The RunError shape for a provider error code reported by the SDK (on an
// assistant message's `error` or an API-error result), or undefined for any
// other code.
export function providerRunError(code, message) {
  if (!PROVIDER_ERRORS.has(code)) return undefined
  return { code, message: String(message ?? code) }
}

// True when the run failed at the provider before the reviewer did any work,
// so retrying with the next API key can help.
export function shouldTryNextKey({ status, error, tools, tokens, findings }) {
  return (
    findings === undefined &&
    status === 'error' &&
    tools === 0 &&
    tokens === 0 &&
    providerRunError(error?.code, error?.message) !== undefined
  )
}

// How a review ends, from the last turn's status and error plus the tool calls
// and tokens of every turn. `findings` stays undefined until post_findings
// records them. This job is advisory, so a provider that failed before the
// reviewer did any work skips the review instead of failing the PR.
export function reviewOutcome({ status, error, tools, tokens, findings }) {
  if (findings !== undefined) return { kind: 'post' }
  const cause = `${error?.code ?? 'unknown'}: ${error?.message ?? 'no error details'}`
  if (status === 'error' && tools === 0 && tokens === 0) {
    return { kind: 'skip', message: `Thermo Review could not run: ${cause}` }
  }
  const detail = error === undefined ? '' : ` ${cause}`
  return { kind: 'fail', message: `Run ended (${status}) without calling post_findings.${detail}` }
}

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'playwright/test'
import { chatTab, launchChat, openWorkspace, selectProject } from './ui'

/**
 * The fake agent's background-work and sub-agent-start prompt markers:
 *
 * - `[BG_SHELL:s]` sends a `tool_call` with `_meta.terminal_info` and a
 *   final `tool_call_update` with no `terminal_exit` (Claude
 *   `run_in_background`), then the exit `s` seconds after the turn ends.
 * - `[BG_OPEN:s]` leaves a `tool_call` `in_progress` at `end_turn` (Codex /
 *   Droid) and completes it `s` seconds later.
 * - `[UNPROMPTED:s]` sends a plain `agent_message_chunk` `s` seconds after
 *   `end_turn`, firing the host's `agent_working` until it goes quiet.
 * - `[SPAWN:fake-longrun:<task>]` calls the host's injected `troupe` MCP
 *   server (`subagent_start`), so a real sub-agent run shows in the chat.
 *
 * Every case asserts the agent's wire frames from WIRE_LOG (the spec reads
 * the seeded `fake-wire.log`, `OUT:` lines are the agent's outbound frames)
 * and the visible UI: the status rail's Background segment, the composer's
 * Stop/Send toggle, or the sub-agent row's Working → Done.
 *
 * Chats live in proj-c, the least contested seeded project (see
 * elicitation.spec.ts).
 */

test.setTimeout(180_000)

const WIRE_LOG_PATH = (): string => {
  const stateDir = process.env.E2E_STATE_DIR
  if (!stateDir) throw new Error('E2E_STATE_DIR unset — run under tests/e2e global-setup')
  return join(stateDir, 'fake-wire.log')
}

/** Wire-log bytes added since `offset` — the log is shared by every suite. */
function wireLogSince(offset: number): string {
  return readFileSync(WIRE_LOG_PATH(), 'utf8').slice(offset)
}

function wireLogOffset(): number {
  try {
    return readFileSync(WIRE_LOG_PATH(), 'utf8').length
  } catch {
    return 0
  }
}

test.beforeEach(async ({ page }) => {
  await openWorkspace(page)
  await selectProject(page, 'proj-c')
})

test('[BG_SHELL] reports a background command that ends on terminal_exit', async ({ page }) => {
  const logOffset = wireLogOffset()
  const title = 'marker bg shell'
  await launchChat(page, `${title} [BG_SHELL:6]`)
  const pane = page.locator('[data-pane-content]').filter({ has: chatTab(page, title) })

  const segment = pane.getByRole('button', { name: /^Background commands: / })
  await expect(segment).toHaveAccessibleName(/^Background commands: 1 running$/)
  await expect(segment).toHaveAccessibleName(/^Background commands: 1 ended$/, {
    timeout: 20_000
  })

  const log = wireLogSince(logOffset)
  // Claude `run_in_background` shape: terminal announced, final status
  // without an exit, then `terminal_exit` after the turn.
  const endIdx = log.indexOf('"stopReason":"end_turn"')
  expect(log).toContain('"terminal_info"')
  expect(endIdx).toBeGreaterThanOrEqual(0)
  expect(log.indexOf('"terminal_exit"')).toBeGreaterThan(endIdx)
})

test('[BG_OPEN] keeps an open tool call running after its turn ends', async ({ page }) => {
  const logOffset = wireLogOffset()
  const title = 'marker bg open'
  await launchChat(page, `${title} [BG_OPEN:6]`)
  const pane = page.locator('[data-pane-content]').filter({ has: chatTab(page, title) })

  const segment = pane.getByRole('button', { name: /^Background commands: / })
  await expect(segment).toHaveAccessibleName(/^Background commands: 1 running$/)
  await expect(segment).toHaveAccessibleName(/^Background commands: 1 ended$/, {
    timeout: 20_000
  })

  const log = wireLogSince(logOffset)
  const endIdx = log.indexOf('"stopReason":"end_turn"')
  expect(endIdx).toBeGreaterThanOrEqual(0)
  // The open call's final status lands after the turn's `end_turn`.
  expect(log.indexOf('bg-open-')).toBeGreaterThanOrEqual(0)
  expect(log.indexOf('"status":"completed"', endIdx)).toBeGreaterThan(endIdx)
})

test('[UNPROMPTED] agent work outside the turn toggles the composer Stop', async ({ page }) => {
  const logOffset = wireLogOffset()
  const title = 'marker unprompted'
  await launchChat(page, `${title} [UNPROMPTED:3]`)
  const pane = page.locator('[data-pane-content]').filter({ has: chatTab(page, title) })

  const send = pane.getByRole('button', { name: 'Send message' })
  const stop = pane.getByRole('button', { name: 'Cancel turn' })
  // Wait on the wire frame, not the button: 'Send message' renders
  // (disabled) before the host's working flag lands, so button-only waits
  // can race through the whole sequence before the 3 s timer fires.
  await expect
    .poll(() => wireLogSince(logOffset).includes('Unprompted update'), { timeout: 15_000 })
    .toBe(true)
  await expect(stop).toBeVisible()
  // The host's quiet window returns the composer to Send.
  await expect(send).toBeVisible({ timeout: 60_000 })

  const log = wireLogSince(logOffset)
  const endIdx = log.indexOf('"stopReason":"end_turn"')
  expect(endIdx).toBeGreaterThanOrEqual(0)
  expect(log.indexOf('Unprompted update')).toBeGreaterThan(endIdx)
})

test('[SPAWN] starts a real sub-agent through the host MCP server', async ({ page }) => {
  const logOffset = wireLogOffset()
  const title = 'marker spawn'
  await launchChat(page, `${title} [SPAWN:fake-longrun:[DURATION:5]]`)
  const pane = page.locator('[data-pane-content]').filter({ has: chatTab(page, title) })

  // The tool result JSON lands in the transcript once the run is up (the
  // reply JSON sits escaped inside the MCP content text, so match the key
  // without its quotes).
  await expect(
    pane
      .getByRole('log')
      .getByText(/SPAWN_RESULT=\{.*subagentId/)
      .filter({ visible: true })
  ).toBeVisible({ timeout: 60_000 })

  // The orchestrated run's row works, then settles when the child's
  // [DURATION:5] turn ends.
  const pill = pane.getByRole('button', { name: /^Subagents: / })
  await expect(pill).toHaveAccessibleName(/1 working/)
  await expect(pill).toHaveAccessibleName(/1 done/, { timeout: 90_000 })

  const log = wireLogSince(logOffset)
  expect(log).toContain('"method":"tools/call"')
  expect(log).toContain('"name":"subagent_start"')
  expect(log).toContain('"agent":"fake-longrun"')
  expect(log).toContain('"workspace":"shared"')
})

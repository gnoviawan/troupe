import { expect, test } from 'playwright/test'
import { E2E_TOKEN } from './helpers'
import {
  launcherComposerGroup,
  launcherContextRow,
  openWorkspace,
  selectProject,
  startNewChat
} from './ui'

/**
 * Worktree-launch E2E on termul-server: New chat in a project opens the agent
 * launcher (#1943), and the context row under its composer must offer the
 * same worktree isolation the desktop has ("New worktree"). The first send in
 * that mode creates a real `git worktree` + a chat in it.
 *
 * Root cause this suite guards (2026-10-06): `ProjectSummary` carries no git
 * fields and the worktree reconciler early-returns on web, so
 * `project.isGitRepo` stayed undefined → `canUseWorktree` false → the
 * isolation picker never rendered. `useProjectGitBranch` now stamps
 * `isGitRepo: true` when `git/commit-context` resolves, and the project
 * mirror carries the flag across `projects_changed` refetches.
 *
 * The fake long-run agent (fake-longrun-agent.ts) serves as the chat agent —
 * a worktree launch still spawns it in the worktree cwd.
 */

test.setTimeout(120_000)

const PROMPT_W = 'delta survey of things'

test.beforeEach(async ({ page }) => {
  await openWorkspace(page)
})

test('the launcher shows the worktree isolation picker for a git-repo project (web parity with desktop)', async ({
  page
}) => {
  await selectProject(page, 'proj-w')

  const composer = await startNewChat(page)
  // New chat in a project opens the agent launcher (#1943).
  await expect(composer).toHaveAttribute('aria-label', 'Agent prompt')
  // The context row becomes interactive once `canUseWorktree` flips true:
  // the git-branch probe resolves asynchronously after project load, so
  // poll for the Workspace select.
  const row = launcherContextRow(page)
  const workspace = row.getByRole('combobox', { name: 'Workspace' })
  await expect(workspace).toBeVisible({ timeout: 10_000 })
  // Radix Select renders options only while open — assert via the trigger's
  // selected value instead (defaults to Current checkout).
  await expect(workspace).toContainText('Current checkout')
})

test('the launcher shows no context row for a non-git project', async ({ page }) => {
  await selectProject(page, 'proj-b')

  const composer = await startNewChat(page)
  await expect(composer).toBeVisible()
  await expect(composer).toHaveAttribute('aria-label', 'Agent prompt')
  // Give the (absent) git probe a beat to settle before asserting absence.
  await page.waitForTimeout(1_500)

  // A project that is not a git repo has no context row (#1856): no
  // workspace picker, no read-only workspace or branch label.
  const group = launcherComposerGroup(page)
  await expect(group).toBeVisible()
  await expect(launcherContextRow(page)).toHaveCount(0)
  await expect(group.getByRole('combobox', { name: 'Workspace' })).toHaveCount(0)
  await expect(group).not.toContainText('Current checkout')
  await expect(group).not.toContainText('Branch:')
})

test('a first send in New worktree mode creates the worktree and opens the chat in it', async ({
  page
}) => {
  await selectProject(page, 'proj-w')

  const composer = await startNewChat(page)
  await expect(composer).toHaveAttribute('aria-label', 'Agent prompt')
  const row = launcherContextRow(page)
  const workspace = row.getByRole('combobox', { name: 'Workspace' })
  await expect(workspace).toBeVisible({ timeout: 10_000 })

  // Select worktree isolation. The row is a radix Select — open it and pick
  // the "New worktree" option. The pick closes the dropdown; wait for that
  // (a left-open dropdown would swallow the Enter below as an option pick).
  // No Escape: in the launcher overlay an Escape that reaches the dialog
  // closes the launcher.
  await workspace.click()
  await page.getByRole('option', { name: 'New worktree' }).click()
  await expect(page.getByRole('listbox')).toHaveCount(0)
  await expect(workspace).toContainText('New worktree')

  // Base-branch picker appears in worktree mode; the resolved default (main)
  // auto-fills, so it is present and non-empty.
  const baseBranch = row.getByRole('combobox', { name: 'Base branch' })
  await expect(baseBranch).toBeVisible()
  await expect(baseBranch).toContainText(/\S/)

  // Compose the first message — the first send runs `git worktree add`, then
  // the chat starts again in the worktree (the in-timeline progress card covers the wait).
  await composer.click()
  await page.keyboard.type(PROMPT_W)
  await expect(composer).toHaveText(PROMPT_W)
  await page.keyboard.press('Enter')

  // The worktree-creation progress card covers the wait, but on a fast host
  // the whole create can complete before the first poll — so accept either
  // the in-progress or the done title, and never require the transient
  // "Creating" frame specifically.
  await expect(page.getByText(/Creating worktree|Worktree created/)).toBeVisible({
    timeout: 30_000
  })
  // The chat tab appears (session created in the worktree cwd).
  const tab = page.getByRole('tab', { name: new RegExp(`^${PROMPT_W}`) }).first()
  await expect(tab).toBeVisible({ timeout: 30_000 })

  // The fake agent streams chunks — the turn runs against the worktree cwd.
  await expect
    .poll(async () => page.locator('body').innerText(), { timeout: 30_000 })
    .toContain('chunk-1 ')

  // Durable proof of the created worktree: the project's git worktree list
  // (via the server route) contains a chat/ branch born from this launch.
  // Bearer token: the route is auth-gated and `page.request` shares the
  // browser context's cookies, not the app's Authorization header.
  const res = await page.request.post(`/worktree/list`, {
    headers: { Authorization: `Bearer ${E2E_TOKEN}` },
    data: { projectPath: `${process.env.E2E_WORKSPACE_ROOT}/proj-w` }
  })
  expect(res.ok()).toBe(true)
  const body = await res.json()
  expect(body.success).toBe(true)
  const branches = (body.data ?? []).map((w: { branch: string }) => w.branch)
  expect(branches.some((b: string) => b.startsWith('chat/'))).toBe(true)
})

test('/health reports write admission so the picker gate reflects server policy', async ({
  request
}) => {
  // The e2e server binds loopback: writes are admitted without the
  // --allow-remote-writes opt-in (mirrors check_local_only admission).
  const res = await request.get('/health')
  expect(res.ok()).toBe(true)
  const body = await res.json()
  expect(body.status).toBe('ok')
  expect(body.allowRemoteWrites).toBe(true)
})

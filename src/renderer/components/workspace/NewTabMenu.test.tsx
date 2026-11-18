import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  resetNativeWebviewOcclusionForTests,
  useNativeWebviewOccluded
} from '@/hooks/use-native-webview-occlusion'
import { NewTabMenu } from './NewTabMenu'
import { type NewTabActions, NewTabActionsProvider } from './new-tab-actions'

const { tauriRef, projectRef, openNewChat, getShells } = vi.hoisted(() => ({
  tauriRef: { current: true as boolean },
  projectRef: {
    current: undefined as { id: string; path?: string; defaultShell?: string } | undefined
  },
  openNewChat: vi.fn(),
  getShells: vi.fn()
}))

vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: () => tauriRef.current }))
vi.mock('@/stores/project-store', () => ({ useActiveProject: () => projectRef.current }))
vi.mock('@/stores/app-settings-store', () => ({ useDefaultShell: () => 'bash' }))
vi.mock('@/components/shell/use-shortcut-label', () => ({ useShortcutLabel: () => undefined }))
vi.mock('@/components/chat/new-chat/open-new-chat', () => ({ openNewChat }))
vi.mock('@/lib/api', () => ({ shellApi: { getAvailableShells: getShells } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

const SHELLS = {
  available: [
    { name: 'zsh', displayName: 'Zsh', path: '/bin/zsh' },
    { name: 'bash', displayName: 'Bash', path: '/bin/bash' }
  ]
}

function Occluded(): React.JSX.Element {
  return <span data-testid="occluded">{String(useNativeWebviewOccluded())}</span>
}

function renderMenu(actions: NewTabActions | null) {
  return render(
    <NewTabActionsProvider value={actions}>
      <NewTabMenu paneId="pane-1" />
      <Occluded />
    </NewTabActionsProvider>
  )
}

function makeActions(): NewTabActions {
  return { addTerminal: vi.fn(), addBrowserTab: vi.fn() }
}

async function openMenu(): Promise<void> {
  fireEvent.pointerDown(screen.getByRole('button', { name: 'New tab' }), {
    button: 0,
    ctrlKey: false
  })
  await screen.findByRole('menu')
}

const itemLabels = () =>
  screen.getAllByRole('menuitem').map((item) => item.textContent?.replace('Default', '').trim())

describe('NewTabMenu', () => {
  beforeEach(() => {
    tauriRef.current = true
    projectRef.current = { id: 'p1', path: '/proj' }
    getShells.mockResolvedValue({ success: true, data: SHELLS })
    openNewChat.mockReset()
  })

  afterEach(() => {
    cleanup()
    resetNativeWebviewOcclusionForTests()
  })

  it('renders one New tab trigger with menu semantics', () => {
    renderMenu(makeActions())
    const trigger = screen.getByRole('button', { name: 'New tab' })
    expect(trigger).toHaveAttribute('aria-haspopup', 'menu')
    expect(trigger).toHaveAttribute('aria-expanded', 'false')
  })

  it('offers a new chat, a new terminal, another shell and a new browser tab', async () => {
    renderMenu(makeActions())
    await openMenu()
    expect(itemLabels()).toEqual([
      'New chat',
      'New terminal',
      'New terminal with',
      'New browser tab'
    ])
  })

  it('opens each kind in its own pane', async () => {
    const actions = makeActions()
    renderMenu(actions)

    await openMenu()
    fireEvent.click(screen.getByRole('menuitem', { name: 'New chat' }))
    expect(openNewChat).toHaveBeenCalledWith({ paneId: 'pane-1', reason: 'new-tab-menu' })

    await openMenu()
    fireEvent.click(screen.getByRole('menuitem', { name: 'New terminal' }))
    expect(actions.addTerminal).toHaveBeenCalledWith('pane-1')

    await openMenu()
    fireEvent.click(screen.getByRole('menuitem', { name: 'New browser tab' }))
    expect(actions.addBrowserTab).toHaveBeenCalledWith('pane-1')
  })

  it('lists the detected shells with the default first, and opens the picked one', async () => {
    const actions = makeActions()
    renderMenu(actions)
    await openMenu()
    const sub = screen.getByRole('menuitem', { name: 'New terminal with' })
    act(() => sub.focus())
    fireEvent.keyDown(sub, { key: 'ArrowRight' })
    await screen.findByText('Zsh')
    const rows = screen
      .getAllByRole('menuitem')
      .filter(
        (item) =>
          item.closest('[data-radix-menu-content]') !== sub.closest('[data-radix-menu-content]')
      )
      .map((item) => item.textContent)
    expect(rows).toEqual(['BashDefault', 'Zsh'])
    fireEvent.click(screen.getByRole('menuitem', { name: /Zsh/ }))
    expect(actions.addTerminal).toHaveBeenCalledWith('pane-1', SHELLS.available[0])
  })

  it('hides New browser tab off the desktop app (browser tabs are native webviews)', async () => {
    tauriRef.current = false
    renderMenu(makeActions())
    await openMenu()
    expect(screen.queryByRole('menuitem', { name: 'New browser tab' })).toBeNull()
  })

  it('disables New chat without a project folder', async () => {
    projectRef.current = undefined
    renderMenu(makeActions())
    await openMenu()
    expect(screen.getByRole('menuitem', { name: 'New chat' })).toHaveAttribute('data-disabled')
  })

  it('occludes the native browser webview while the menu is open', async () => {
    renderMenu(makeActions())
    expect(screen.getByTestId('occluded').textContent).toBe('false')
    await openMenu()
    await waitFor(() => expect(screen.getByTestId('occluded').textContent).toBe('true'))
  })
})

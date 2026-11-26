import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

interface FakeState {
  updateAvailable: boolean
  version: string | null
  downloaded: boolean
  downloadProgress: number
  skippedVersion: string | null
  isChecking: boolean
  isDownloading: boolean
  isPreparingDownload: boolean
  error: string | null
  errorSource: 'check' | 'download' | 'install' | null
  lastChecked: Date | null
  releaseNotes: string | null
  isManualUpdateMode: boolean
}

const BASE: FakeState = {
  updateAvailable: false,
  version: null,
  downloaded: false,
  downloadProgress: 0,
  skippedVersion: null,
  isChecking: false,
  isDownloading: false,
  isPreparingDownload: false,
  error: null,
  errorSource: null,
  lastChecked: null,
  releaseNotes: null,
  isManualUpdateMode: false
}

const stateRef = vi.hoisted(() => ({ current: {} as Record<string, unknown> }))
const actions = vi.hoisted(() => ({
  checkForUpdates: vi.fn(),
  downloadUpdate: vi.fn(async () => {}),
  installAndRestart: vi.fn(async () => {}),
  skipVersion: vi.fn(async () => {})
}))
const confirmInstall = vi.hoisted(() => vi.fn(async () => {}))

vi.mock('@/stores/updater-store', () => ({
  useUpdaterState: () => stateRef.current,
  useUpdaterActions: () => actions
}))
vi.mock('@/lib/confirm-install-update', () => ({ confirmInstallAndRestart: confirmInstall }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }))

import { VersionCard } from './VersionCard'

function renderCard(state: Partial<FakeState> = {}, isAurUpdater = false): void {
  stateRef.current = { ...BASE, ...state }
  render(<VersionCard isAurUpdater={isAurUpdater} />)
}

beforeEach(() => {
  for (const action of Object.values(actions)) action.mockClear()
  confirmInstall.mockClear()
})

describe('VersionCard', () => {
  it('offers Check for updates before the first check', () => {
    renderCard()
    expect(screen.getByText('Not checked yet')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }))
    expect(actions.checkForUpdates).toHaveBeenCalled()
  })

  it('says up to date after a check that found nothing', () => {
    renderCard({ lastChecked: new Date('2026-11-25T10:00:00') })
    expect(screen.getByText(/^Up to date · last checked/)).toBeInTheDocument()
  })

  it('disables the action with a spinner while checking', () => {
    renderCard({ isChecking: true })
    expect(screen.getByRole('button', { name: 'Checking…' })).toBeDisabled()
  })

  it('offers Download and Skip this version for a signed update', () => {
    renderCard({ updateAvailable: true, version: '0.5.1', releaseNotes: 'Faster chats' })
    expect(screen.getByText('Version 0.5.1 is available.')).toBeInTheDocument()
    expect(screen.getByText("What's new: Faster chats")).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Download' }))
    expect(actions.downloadUpdate).toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Skip this version' }))
    expect(actions.skipVersion).toHaveBeenCalledWith('0.5.1')
  })

  it('shows the backup step, then the download progress', () => {
    renderCard({ updateAvailable: true, version: '0.5.1', isPreparingDownload: true })
    expect(
      screen.getByText('Backing up your Troupe data before the download starts.')
    ).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Preparing…' })).toBeDisabled()
  })

  it('shows a progress bar while downloading', () => {
    renderCard({
      updateAvailable: true,
      version: '0.5.1',
      isDownloading: true,
      downloadProgress: 42
    })
    expect(screen.getByText('Version 0.5.1 · 42% complete')).toBeInTheDocument()
    expect(screen.getByRole('progressbar', { name: 'Download progress' })).toHaveAttribute(
      'aria-valuenow',
      '42'
    )
  })

  it('installs through the confirm once downloaded', () => {
    renderCard({ updateAvailable: true, version: '0.5.1', downloaded: true })
    fireEvent.click(screen.getByRole('button', { name: 'Install and restart' }))
    expect(confirmInstall).toHaveBeenCalledWith('0.5.1')
  })

  it('opens the download page in manual mode', () => {
    renderCard({ updateAvailable: true, version: '0.5.1', isManualUpdateMode: true })
    fireEvent.click(screen.getByRole('button', { name: 'Open download page' }))
    expect(actions.installAndRestart).toHaveBeenCalled()
  })

  it('shows the AUR command and no button for an AUR install', () => {
    renderCard({ updateAvailable: true, version: '0.5.1' }, true)
    expect(screen.getByText('yay -S troupe')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Download' })).not.toBeInTheDocument()
  })

  it.each([
    ['check', 'Could not check', 'checkForUpdates'],
    ['download', 'Download failed', 'downloadUpdate']
  ] as const)('retries only the step that failed (%s)', (source, status, action) => {
    renderCard({ version: '0.5.1', updateAvailable: true, error: 'boom', errorSource: source })
    expect(screen.getByText(status)).toBeInTheDocument()
    expect(screen.getByText('boom')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }))
    expect(actions[action]).toHaveBeenCalled()
  })

  it('retries a failed install through the confirm', () => {
    renderCard({
      version: '0.5.1',
      updateAvailable: true,
      downloaded: true,
      error: 'gone',
      errorSource: 'install'
    })
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }))
    expect(confirmInstall).toHaveBeenCalledWith('0.5.1')
  })

  it('notes a skipped version quietly', () => {
    renderCard({ skippedVersion: '0.5.1', lastChecked: new Date() })
    expect(screen.getByText(/You skipped version/)).toBeInTheDocument()
  })
})

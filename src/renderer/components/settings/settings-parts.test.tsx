import { act, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { ProgressBar } from '@/components/ui/progress-bar'
import { SettingsDisclosure } from './SettingsDisclosure'
import { SettingsFilter } from './SettingsFilter'
import { SETTINGS_REVEAL_EVENT } from './SettingsLayout'
import { SettingsSegmented } from './SettingsSegmented'

const CHANNELS = [
  { value: 'stable', label: 'Stable' },
  { value: 'nightly', label: 'Nightly' }
] as const

function Channel(): React.JSX.Element {
  const [value, setValue] = useState<'stable' | 'nightly'>('stable')
  return (
    <SettingsSegmented
      label="Release channel"
      options={CHANNELS}
      value={value}
      onValueChange={setValue}
    />
  )
}

describe('SettingsSegmented', () => {
  it('is a radio group with one tab stop on the chosen option', () => {
    render(<Channel />)

    expect(screen.getByRole('radiogroup', { name: 'Release channel' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Stable' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'Stable' })).toHaveAttribute('tabindex', '0')
    expect(screen.getByRole('radio', { name: 'Nightly' })).toHaveAttribute('tabindex', '-1')
  })

  it('moves the choice with a click and with the arrow keys', () => {
    render(<Channel />)

    fireEvent.click(screen.getByRole('radio', { name: 'Nightly' }))
    expect(screen.getByRole('radio', { name: 'Nightly' })).toHaveAttribute('aria-checked', 'true')

    fireEvent.keyDown(screen.getByRole('radiogroup'), { key: 'ArrowRight' })
    expect(screen.getByRole('radio', { name: 'Stable' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'Stable' })).toHaveFocus()
  })

  it('shows a count after the label', () => {
    render(
      <SettingsSegmented
        label="Scope"
        options={[{ value: 'all', label: 'All', count: 191 }]}
        value="all"
        onValueChange={vi.fn()}
      />
    )

    expect(screen.getByRole('radio', { name: 'All 191' })).toBeInTheDocument()
  })
})

describe('SettingsFilter', () => {
  it('clears its text with the clear button', () => {
    const onValueChange = vi.fn()
    render(<SettingsFilter label="Filter shortcuts" value="zoom" onValueChange={onValueChange} />)

    fireEvent.click(screen.getByRole('button', { name: 'Clear filter' }))
    expect(onValueChange).toHaveBeenCalledWith('')
  })
})

describe('SettingsDisclosure', () => {
  it('starts closed with its summary, keeps its rows mounted, and opens on click', () => {
    render(
      <SettingsDisclosure title="Advanced" summary="Auto · 10,000 lines">
        <p>Renderer row</p>
      </SettingsDisclosure>
    )

    const trigger = screen.getByRole('button', { name: /Advanced/ })
    expect(trigger).toHaveAttribute('aria-expanded', 'false')
    expect(screen.getByText('Auto · 10,000 lines')).toBeInTheDocument()
    expect(screen.getByText('Renderer row')).not.toBeVisible()

    fireEvent.click(trigger)
    expect(trigger).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByText('Renderer row')).toBeVisible()
    expect(screen.queryByText('Auto · 10,000 lines')).not.toBeInTheDocument()
  })
})

describe('SettingsDisclosure search reveal', () => {
  it('opens when a search jump targets a row inside it', () => {
    render(
      <SettingsDisclosure title="Advanced" summary="Auto">
        <p id="setting-terminal-renderer">Renderer row</p>
      </SettingsDisclosure>
    )

    const row = screen.getByText('Renderer row')
    expect(row).not.toBeVisible()
    act(() => {
      row.dispatchEvent(new CustomEvent(SETTINGS_REVEAL_EVENT, { bubbles: true }))
    })
    expect(row).toBeVisible()
  })
})

describe('ProgressBar', () => {
  it('reports a determinate value', () => {
    render(<ProgressBar label="Download" value={42} valueText="42% complete" />)

    const bar = screen.getByRole('progressbar', { name: 'Download' })
    expect(bar).toHaveAttribute('aria-valuenow', '42')
    expect(bar).toHaveAttribute('aria-valuetext', '42% complete')
  })

  it('has no value while indeterminate', () => {
    render(<ProgressBar label="Preparing" />)

    expect(screen.getByRole('progressbar', { name: 'Preparing' })).not.toHaveAttribute(
      'aria-valuenow'
    )
  })
})

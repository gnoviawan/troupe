import { act, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { ProgressBar } from '@/components/ui/progress-bar'
import { catalogColumnsForWidth } from './McpCatalogCard'
import { SettingsDisclosure } from './SettingsDisclosure'
import { SettingsFilter } from './SettingsFilter'
import { SETTINGS_SPLIT_GROUP_CLASS, SettingsGroup } from './SettingsGroup'
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

    fireEvent.click(screen.getByRole('button', { name: 'Clear text' }))
    expect(onValueChange).toHaveBeenCalledWith('')
  })
})

describe('SettingsGroup', () => {
  it('renders title, description, meta, actions, children and footnote in split mode by default', () => {
    render(
      <SettingsGroup
        title="Appearance"
        description="Theme, interface zoom, and window chrome."
        meta="3 options"
        actions={<button type="button">Reset</button>}
        footnote="Applies immediately."
      >
        <p>Row content</p>
      </SettingsGroup>
    )

    const section = screen.getByRole('region', { name: 'Appearance' })
    expect(section).toHaveClass(SETTINGS_SPLIT_GROUP_CLASS)
    expect(screen.getByText('Theme, interface zoom, and window chrome.')).toBeInTheDocument()
    expect(screen.getByText('3 options')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Reset' })).toBeInTheDocument()
    expect(screen.getByText('Row content')).toBeInTheDocument()
    expect(screen.getByText('Applies immediately.')).toBeInTheDocument()
  })

  it('stays stacked when layout="stacked" or title is omitted', () => {
    const { rerender } = render(
      <SettingsGroup
        title="Added"
        layout="stacked"
        description="Installed servers."
        meta="4 servers"
        actions={<button type="button">Add server</button>}
        footnote="Changes apply to new chats."
      >
        <p>Card grid</p>
      </SettingsGroup>
    )

    const stacked = screen.getByRole('region', { name: 'Added' })
    expect(stacked).not.toHaveClass(SETTINGS_SPLIT_GROUP_CLASS)
    expect(screen.getByText('Installed servers.')).toBeInTheDocument()
    expect(screen.getByText('Card grid')).toBeInTheDocument()
    expect(screen.getByText('Changes apply to new chats.')).toBeInTheDocument()

    rerender(
      <SettingsGroup data-testid="untitled-group">
        <p>Standalone card</p>
      </SettingsGroup>
    )
    const untitled = screen.getByTestId('untitled-group')
    expect(untitled).not.toHaveClass(SETTINGS_SPLIT_GROUP_CLASS)
    expect(untitled).not.toHaveAttribute('aria-labelledby')
  })
})

describe('SettingsDisclosure', () => {
  it('starts closed with its summary, keeps its rows mounted, and opens on click', () => {
    render(
      <SettingsDisclosure
        title="Advanced"
        description="Renderer, scrollback history, and command completions."
        summary="Auto · 10,000 lines"
      >
        <p>Renderer row</p>
      </SettingsDisclosure>
    )

    expect(
      screen.getByText('Renderer, scrollback history, and command completions.')
    ).toBeInTheDocument()
    const trigger = screen.getByRole('button', { name: /Advanced/ })
    expect(trigger).toHaveAttribute('aria-expanded', 'false')
    expect(screen.getByText('Auto · 10,000 lines')).toBeInTheDocument()
    expect(screen.getByText('Renderer row')).not.toBeVisible()

    fireEvent.click(trigger)
    expect(trigger).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByText('Renderer row')).toBeVisible()
    expect(screen.queryByText('Auto · 10,000 lines')).not.toBeInTheDocument()
  })

  it('supports layout="stacked" without the split grid wrapper', () => {
    const { container } = render(
      <SettingsDisclosure title="Advanced" summary="Extra options" layout="stacked">
        <p>Nested row</p>
      </SettingsDisclosure>
    )

    expect(container.firstElementChild).not.toHaveClass(SETTINGS_SPLIT_GROUP_CLASS)
    expect(screen.getByRole('button', { name: /Advanced/ })).toBeInTheDocument()
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

describe('catalogColumnsForWidth', () => {
  it('returns 1 column below 520px, 2 from 520px to 859px, and 3 at 860px or wider', () => {
    expect(catalogColumnsForWidth(320)).toBe(1)
    expect(catalogColumnsForWidth(519)).toBe(1)
    expect(catalogColumnsForWidth(520)).toBe(2)
    expect(catalogColumnsForWidth(859)).toBe(2)
    expect(catalogColumnsForWidth(860)).toBe(3)
    expect(catalogColumnsForWidth(1080)).toBe(3)
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

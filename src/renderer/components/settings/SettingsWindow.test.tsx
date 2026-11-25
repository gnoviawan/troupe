import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsWindow } from './SettingsWindow'

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockImplementation(function (
    this: HTMLElement
  ) {
    if (this.closest('[hidden]')) return [] as unknown as DOMRectList
    return [new DOMRect(0, 0, 10, 10)] as unknown as DOMRectList
  })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('SettingsWindow', () => {
  it('fills the window as a named modal dialog', () => {
    render(
      <SettingsWindow isOpen onClose={() => undefined} title="Preferences">
        <p>Body</p>
      </SettingsWindow>
    )

    const dialog = screen.getByRole('dialog', { name: 'Preferences' })
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(dialog.className).toContain('fixed inset-0')
  })

  it('wraps Tab among visible controls and skips a hidden page', () => {
    render(
      <SettingsWindow isOpen onClose={() => undefined} title="Preferences">
        <button type="button">First</button>
        <section hidden>
          <button type="button">Hidden</button>
        </section>
        <button type="button">Last</button>
      </SettingsWindow>
    )

    const first = screen.getByRole('button', { name: 'First' })
    const last = screen.getByRole('button', { name: 'Last' })
    last.focus()
    fireEvent.keyDown(window, { key: 'Tab' })
    expect(first).toHaveFocus()

    fireEvent.keyDown(window, { key: 'Tab', shiftKey: true })
    expect(last).toHaveFocus()
  })

  it('closes on Escape while a shortcut recorder is only mounted', () => {
    const onClose = vi.fn()
    render(
      <SettingsWindow isOpen onClose={onClose} title="Preferences">
        <div data-shortcut-recorder="true" />
      </SettingsWindow>
    )

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('keeps open on Escape while a shortcut is being recorded', () => {
    const onClose = vi.fn()
    render(
      <SettingsWindow isOpen onClose={onClose} title="Preferences">
        <div data-shortcut-recorder="true" data-recording="true" />
      </SettingsWindow>
    )

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(onClose).not.toHaveBeenCalled()
  })

  it('leaves Escape to an open sibling confirm', () => {
    const onClose = vi.fn()
    render(
      <>
        <SettingsWindow isOpen onClose={onClose} title="Preferences">
          <p>Body</p>
        </SettingsWindow>
        <div data-sibling-dialog />
      </>
    )

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(onClose).not.toHaveBeenCalled()
  })
})

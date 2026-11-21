import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TOUCH_HIT_AREA_CLASS } from '@/components/phone-modal-classes'
import { availableColors } from '@/lib/colors'
import { ColorSwatchGroup } from './ColorSwatchGroup'

describe('ColorSwatchGroup', () => {
  it('renders one named radio per project color', () => {
    render(<ColorSwatchGroup aria-label="Project color" defaultValue="blue" />)

    const swatches = screen.getAllByRole('radio')
    expect(swatches).toHaveLength(availableColors.length)
    expect(screen.getByRole('radio', { name: 'Blue' })).toHaveAttribute('data-state', 'checked')
  })

  it('keeps the 24px swatch but gives each a 44px hit area on a touch screen', () => {
    render(<ColorSwatchGroup aria-label="Project color" />)

    for (const swatch of screen.getAllByRole('radio')) {
      expect(swatch).toHaveClass('size-6')
      for (const token of TOUCH_HIT_AREA_CLASS.split(' ')) {
        expect(swatch).toHaveClass(token)
      }
    }
  })

  it('spaces swatches 20px apart on a touch screen so the hit areas do not overlap', () => {
    render(<ColorSwatchGroup aria-label="Project color" />)

    const group = screen.getByRole('radiogroup', { name: 'Project color' })
    expect(group).toHaveClass('gap-1.5', 'pointer-coarse:gap-5', 'pointer-coarse:py-2.5')
  })
})

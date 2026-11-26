import { expect, test } from 'bun:test'
import { SURFACES } from '../surfaces'
import { measureOverlays, measureScreens } from '../surfaces/inventory'
import { AREA_WEIGHTS } from '../weights'

test('surface ids are unique', () => {
  const ids = SURFACES.map((s) => s.id)
  expect(new Set(ids).size).toBe(ids.length)
})

test('the list holds the measured inventory plus the starter and phone journeys', () => {
  const count = (kind: 'overlay' | 'screen' | 'journey') =>
    SURFACES.filter((s) => kind in s.source).length
  expect({
    overlays: count('overlay'),
    screens: count('screen'),
    journeys: count('journey'),
    phone: SURFACES.filter((s) => s.profile === 'phone').length,
    total: SURFACES.length
  }).toEqual({ overlays: 148, screens: 45, journeys: 29, phone: 37, total: 222 })
})

test('the surfaces claim every overlay mount site of the renderer, once', () => {
  const claimed: Record<string, number> = {}
  for (const surface of SURFACES) {
    if (!('overlay' in surface.source)) continue
    for (const file of surface.source.overlay) claimed[file] = (claimed[file] ?? 0) + 1
  }
  // A failure names the file whose mount sites changed: add or drop a surface for it.
  expect(claimed).toEqual(measureOverlays())
})

test('the surfaces claim every screen of the renderer, once', () => {
  const claimed: Record<string, string[]> = {}
  for (const surface of SURFACES) {
    if (!('screen' in surface.source)) continue
    const { screen, name } = surface.source
    claimed[screen] = [...(claimed[screen] ?? []), name].sort()
  }
  expect(claimed).toEqual(measureScreens())
})

test('every phone journey names a spec under tests/e2e', async () => {
  const specs = [
    ...new Bun.Glob('tests/e2e/mobile-*.spec.ts').scanSync(`${import.meta.dir}/../../..`)
  ]
  const journeys = SURFACES.flatMap((s) =>
    s.profile === 'phone' && 'journey' in s.source ? [s.source.journey] : []
  )
  expect(journeys.sort()).toEqual(specs.sort())
})

test('every surface is driven or says why not, with unique step names and a weighted area', () => {
  for (const surface of SURFACES) {
    const names = surface.steps.map((s) => s.name)
    expect({
      id: surface.id,
      driven: names.length > 0 || Boolean(surface.blocked ?? surface.pending),
      unique: new Set(names).size === names.length
    }).toEqual({ id: surface.id, driven: true, unique: true })
    expect(AREA_WEIGHTS[surface.area]).toBeGreaterThan(0)
  }
})

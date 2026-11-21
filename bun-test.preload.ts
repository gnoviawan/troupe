import { vi } from 'vitest'

// Vitest treats clearAllTimers as a no-op under real timers; bun throws.
const clearAllTimers = vi.clearAllTimers.bind(vi)
vi.clearAllTimers = () => {
  if (vi.isFakeTimers()) clearAllTimers()
  return vi
}

await import('./vitest.setup')

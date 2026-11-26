import {
  readOverlaySentinelDepth,
  useOverlayStackStore
} from '../../../../src/renderer/stores/overlay-stack-store'
import { emit, type Scenario, type StoreRecorder, type VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const q = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

export const overlayScenarios: Scenario[] = [
  {
    name: 'register_and_close_topmost',
    steps: [
      ev('closeTopmostOverlay'),
      ev('registerOverlay', 'mobile-drawer'),
      ev('registerOverlay', 'git-sheet'),
      ev('registerOverlay', 'mobile-drawer'),
      ev('closeTopmostOverlay'),
      ev('unregisterOverlay', 'git-sheet'),
      ev('unregisterOverlay', 'git-sheet'),
      ev('closeTopmostOverlay'),
      ev('unregisterOverlay', 'mobile-drawer'),
      ev('closeTopmostOverlay')
    ]
  },
  {
    name: 'mobile_shell_flag',
    steps: [
      ev('setMobileShell', true),
      ev('setMobileShell', true),
      ev('registerOverlay', 'settings-modal'),
      ev('setMobileShell', false)
    ]
  },
  {
    name: 'sentinel_depth',
    steps: [
      null,
      'route',
      { termulOverlay: true },
      { termulOverlay: true, termulOverlayDepth: 3 },
      { termulOverlay: true, termulOverlayDepth: 0 },
      { termulOverlay: true, termulOverlayDepth: 2.5 },
      { termulOverlay: true, termulOverlayDepth: '2' },
      { termulOverlay: 'true', termulOverlayDepth: 2 },
      { idx: 4, termulOverlay: true, termulOverlayDepth: 7 }
    ].map((state) => q('readOverlaySentinelDepth', state))
  }
]

export const overlayRecorder: StoreRecorder = {
  store: 'overlay',
  sources: ['src/renderer/stores/overlay-stack-store.ts'],
  reset() {
    useOverlayStackStore.setState({ stack: [], mobileShell: false })
  },
  seed(initial) {
    useOverlayStackStore.setState(structuredClone(initial))
  },
  snapshot() {
    const { stack, mobileShell } = useOverlayStackStore.getState()
    return { stack: stack.map((entry) => entry.id), mobileShell }
  },
  dispatch({ action, args }) {
    if (action === 'readOverlaySentinelDepth') return readOverlaySentinelDepth(args[0])
    const store = useOverlayStackStore.getState()
    switch (action) {
      // The owner's close is recorded, not run: its own unregister follows as an event.
      case 'registerOverlay': {
        const id = args[0] as string
        return store.registerOverlay(id, () => emit({ kind: 'closeOverlay', id }))
      }
      case 'unregisterOverlay':
        return store.unregisterOverlay(args[0] as string)
      case 'setMobileShell':
        return store.setMobileShell(args[0] as boolean)
      case 'closeTopmostOverlay':
        return store.closeTopmostOverlay()
      default:
        throw new Error(`overlay store has no action ${action}`)
    }
  }
}

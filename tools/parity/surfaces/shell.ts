import type { Surface } from '../types'
import { CHAT_SETTLED, click, css, DASHBOARD, label, see, step, wait } from './dsl'

/** A gate replaces the whole workspace; none of them shows on a healthy, set-up web client. */
const gate = (name: string, title: string, blocked: string, reference?: 'tauri'): Surface => ({
  id: `shell.gate-${name.replace(/[A-Z]/g, (c, i) => `${i ? '-' : ''}${c.toLowerCase()}`)}`,
  area: 'dialogs',
  title,
  source: { screen: 'gate', name },
  ...(reference ? { reference } : {}),
  blocked,
  steps: []
})

export const SHELL: Surface[] = [
  {
    id: 'shell.dashboard',
    area: 'rail',
    title: 'Workspace dashboard',
    source: { screen: 'route', name: '/' },
    steps: [DASHBOARD]
  },
  {
    id: 'shell.chat-route',
    area: 'chat',
    title: 'Chat route opened from its rail row',
    source: { screen: 'route', name: '/c/:sessionId' },
    steps: [
      DASHBOARD,
      step(
        'b-open',
        [click(css('ul[aria-label="Chats in alpha"] button')), wait(1500)],
        CHAT_SETTLED
      )
    ]
  },
  {
    id: 'shell.not-found',
    area: 'dialogs',
    title: 'Page not found',
    source: { screen: 'route', name: '*' },
    start: { hash: '#/nowhere', ready: 'Page not found' },
    steps: [step('a-not-found', [], see('Page not found', 'Return to Workspace'))]
  },
  {
    id: 'shell.canvas',
    area: 'canvas',
    title: 'Design canvas tab',
    source: { screen: 'pane', name: 'canvas' },
    steps: [
      DASHBOARD,
      step('b-canvas', [click(label('Open canvas')), wait(2500)], see('design.figcraft'))
    ]
  },
  {
    id: 'shell.canvas-unsaved',
    area: 'canvas',
    title: 'Canvas not saved on close',
    source: { overlay: ['components/canvas/CanvasUnmountDialog.tsx'] },
    blocked:
      'opens only when a canvas fails to save as its project closes; the seeded host saves it',
    steps: []
  },
  {
    id: 'shell.browser',
    area: 'browser',
    title: 'Browser tab',
    source: { screen: 'pane', name: 'browser' },
    reference: 'tauri',
    blocked: 'browser tabs are native webviews in the desktop app; the web client cannot open one',
    steps: []
  },
  gate(
    'WebTokenGateScreen',
    'Web access token gate',
    'the matrix signs every client in with the seeded token, so the token gate never shows'
  ),
  gate(
    'ServerSetupWizard',
    'Server setup wizard',
    'shows only in the desktop app before a server is configured',
    'tauri'
  ),
  gate(
    'RestartingScreen',
    'Restarting screen',
    'shows only while the desktop app restarts its server',
    'tauri'
  ),
  gate(
    'ErrorFallback',
    'Crash screen',
    'shows only after the renderer throws, which no seeded step causes'
  ),
  gate(
    'Onboarding',
    'First-run onboarding',
    'shows only on a first run with no projects; the seeded world has four'
  )
]

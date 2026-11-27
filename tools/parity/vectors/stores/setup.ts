import {
  type OnboardingChoice,
  onboardingLanding
} from '../../../../src/renderer/components/onboarding/land-onboarding'
import {
  decideOnboarding,
  type OnboardingFacts
} from '../../../../src/renderer/components/onboarding/onboarding-gate'
import {
  restartHealthUrl,
  restartLandingUrl
} from '../../../../src/renderer/components/setup/RestartingScreen'
import { normalizeSetupHost } from '../../../../src/renderer/components/setup/ServerSetupWizard'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * First run's pure rules: the wizard's host spelling, the restart handoff's health and landing
 * URLs (the page's host is jsdom's `localhost`), the onboarding gate's decision and where
 * onboarding lands. `tests/setup` replays each as a read against `model::setup`.
 */
export const setupRecorder: StoreRecorder = {
  store: 'setup',
  sources: [
    'src/renderer/components/setup/ServerSetupWizard.tsx',
    'src/renderer/components/setup/RestartingScreen.tsx',
    'src/renderer/components/onboarding/onboarding-gate.ts',
    'src/renderer/components/onboarding/land-onboarding.ts'
  ],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    if (action === 'normalizeHost') return normalizeSetupHost(args[0] as string)
    if (action === 'healthUrl') return restartHealthUrl(args[0] as string, args[1] as number)
    if (action === 'landingUrl')
      return restartLandingUrl(args[0] as string, args[1] as number, args[2] as string | undefined)
    if (action === 'decide') return decideOnboarding(args[0] as OnboardingFacts)
    if (action === 'landing') return onboardingLanding(args[0] as OnboardingChoice)
    throw new Error(`setup has no action ${action}`)
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

const facts = (over: Partial<OnboardingFacts>): OnboardingFacts => ({
  hostRead: 'missing',
  completedAt: null,
  projectsLoaded: true,
  projectCount: 0,
  writes: 'admitted',
  ...over
})

const agent = { id: 'claude-acp' } as OnboardingChoice['agent']

export const setupScenarios: Scenario[] = [
  {
    name: 'hosts_read_as_one_of_the_two_bind_modes',
    steps: ['127.0.0.1', '0.0.0.0', ' ALL ', 'any', 'localhost', 'loopback', '', '10.0.0.2'].map(
      (host) => read('normalizeHost', host)
    )
  },
  {
    name: 'the_restart_lands_on_the_new_address_with_the_token',
    steps: [
      read('healthUrl', '127.0.0.1', 8195),
      read('healthUrl', '0.0.0.0', 9000),
      read('landingUrl', '127.0.0.1', 8195),
      read('landingUrl', '0.0.0.0', 8195, 'tok en/+=?&#'),
      read('landingUrl', ' box.lan ', 80, 'ünï')
    ]
  },
  {
    name: 'first_run_shows_only_when_every_fact_holds',
    steps: [
      read('decide', facts({})),
      read('decide', facts({ projectCount: 2 })),
      read('decide', facts({ hostRead: 'failed' })),
      read('decide', facts({ completedAt: '2026-11-20T10:00:00.000Z' })),
      read('decide', facts({ hostRead: 'pending', completedAt: '2026-11-20T10:00:00.000Z' })),
      read('decide', facts({ writes: 'refused' })),
      read('decide', facts({ writes: 'timed-out' })),
      read('decide', facts({ writes: 'pending' })),
      read('decide', facts({ projectsLoaded: false })),
      read('decide', facts({ hostRead: 'found' })),
      read('decide', facts({ hostRead: 'pending' }))
    ]
  },
  {
    name: 'onboarding_lands_on_the_launcher_a_quick_chat_or_the_workspace',
    steps: [
      read('landing', { agent, projectId: 'p1' }),
      read('landing', { agent: null, projectId: 'p1' }),
      read('landing', { agent, projectId: null }),
      read('landing', { agent: null, projectId: null })
    ]
  }
]

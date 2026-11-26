import {
  parseTerminalProfileDraft,
  type TerminalProfileDraft,
  terminalProfileCommandLine,
  terminalProfileToDraft
} from '../../../../src/renderer/lib/terminal-profiles'
import type { TerminalProfile } from '../../../../src/renderer/types/settings'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The Profiles page's pure logic: the editor draft's parse (with the id an edit keeps), a
 * profile read back into a draft, and the command line a row shows. `tests/settings` replays
 * each as a read against `model::settings::profiles`.
 */
export const terminalProfilesRecorder: StoreRecorder = {
  store: 'terminal-profiles',
  sources: ['src/renderer/lib/terminal-profiles.ts'],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    if (action === 'parse')
      return parseTerminalProfileDraft(args[0] as TerminalProfileDraft, args[1] as string)
    if (action === 'toDraft') return terminalProfileToDraft(args[0] as TerminalProfile)
    if (action === 'commandLine') return terminalProfileCommandLine(args[0] as TerminalProfile)
    throw new Error(`terminal-profiles has no action ${action}`)
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})
const draft = (
  name: string,
  shell: string,
  args: string,
  cwd: string,
  env: string
): TerminalProfileDraft => ({
  name,
  shell,
  args,
  cwd,
  env
})
const profile = (
  shell: string,
  args: string[],
  env: Record<string, string> = {}
): TerminalProfile => ({
  id: 'p1',
  name: 'Node dev',
  shell,
  args,
  cwd: '/w',
  env
})

export const terminalProfilesScenarios: Scenario[] = [
  {
    name: 'drafts_parse_or_name_each_bad_field',
    steps: [
      read(
        'parse',
        draft(' Node dev ', 'zsh', '-l\r\n\n-i', ' /w ', '# note\nNODE_ENV=development\n A = b=c '),
        'p1'
      ),
      read('parse', draft('Plain', '', '', '', ''), 'p2'),
      read('parse', draft('  ', '', '-l', '', 'ok=1\n1BAD=x'), 'p3'),
      read('parse', draft('x', 'bash', '', '', 'NOEQUALS'), 'p4'),
      read('parse', draft('x', 'bash', '', '', '\n\n=v'), 'p5'),
      read(
        'parse',
        draft('x', 'pwsh', '-NoProfile', '', '  # spaced comment\r\nA_1=\r\n_b=two words'),
        'p6'
      )
    ]
  },
  {
    name: 'profiles_read_back_and_show_their_command',
    steps: [
      read('toDraft', profile('bash', ['-c', 'echo hi'], { A: '1', B: '2' })),
      read('toDraft', profile('', [])),
      read('commandLine', profile('', [])),
      read('commandLine', profile('/bin/zsh', ['-l', '-i'])),
      read('commandLine', profile('nu', []))
    ]
  }
]

import {
  shellFromSelectValue,
  shellSelectOptions,
  shellSelectValue
} from '../../../../src/renderer/components/settings/shell-options'
import {
  mergeEnvVars,
  parseEnvFile,
  resolveProjectEnvPath
} from '../../../../src/renderer/lib/env-parser'
import type { EnvVariable } from '../../../../src/renderer/types/project'
import type { DetectedShells } from '../../../../src/shared/types/ipc.types'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The project settings window's pure logic: the `.env` path, parse and merge its Import
 * runs, and the shell select's value, options and stored choice. `tests/settings` replays
 * each as a read against `model::settings::project`.
 */
export const projectSettingsRecorder: StoreRecorder = {
  store: 'project-settings',
  sources: ['src/renderer/lib/env-parser.ts', 'src/renderer/components/settings/shell-options.ts'],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    if (action === 'envPath') return resolveProjectEnvPath(args[0] as string)
    if (action === 'parseEnv') return parseEnvFile(args[0] as string)
    if (action === 'mergeEnv')
      return mergeEnvVars(args[0] as EnvVariable[], args[1] as EnvVariable[])
    if (action === 'shellValue')
      return shellSelectValue(args[0] as string, args[1] as DetectedShells | null)
    if (action === 'shellOptions')
      return shellSelectOptions(
        args[0] as string,
        args[1] as DetectedShells | null,
        args[2] as string
      )
    if (action === 'shellFromValue') return shellFromSelectValue(args[0] as string)
    throw new Error(`project-settings has no action ${action}`)
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})
const SHELLS: DetectedShells = {
  default: null,
  available: [
    { name: 'bash', path: '/bin/bash', displayName: 'Bash' },
    { name: 'zsh', path: '/usr/bin/zsh', displayName: 'Zsh' },
    {
      name: 'pwsh',
      path: 'C:\\Program Files\\PowerShell\\7\\pwsh.exe',
      displayName: 'PowerShell 7'
    }
  ]
}
const v = (key: string, value: string, isSecret?: boolean): EnvVariable =>
  isSecret === undefined ? { key, value } : { key, value, isSecret }

export const projectSettingsScenarios: Scenario[] = [
  {
    name: 'env_path_keeps_the_roots_separator',
    steps: [
      '/w/app',
      '/w/app///',
      '',
      '/',
      'C:\\w\\app\\',
      'C:/w/app/',
      'C:\\',
      '\\\\srv\\share\\',
      'rel/dir'
    ].map((root) => read('envPath', root))
  },
  {
    name: 'env_files_parse_line_by_line',
    steps: [
      read('parseEnv', 'A=1\r\nB = two words \n# note\n\n  C="quoted"\nD=\'single\'\nE="\nF=a=b=c'),
      read('parseEnv', 'NOEQUALS\n=v\n1BAD=x\nok_2=\n  spaced key=1\nG="half\nH=\'mixed"'),
      read('parseEnv', ''),
      read('parseEnv', '   \n# only\n\t#tabbed')
    ]
  },
  {
    name: 'imports_merge_over_the_list',
    steps: [
      read('mergeEnv', [v('A', '1'), v('B', '2', true)], [v('B', 'new'), v('C', '3')]),
      read('mergeEnv', [v('A', '1'), v('A', '2')], []),
      read('mergeEnv', [], [v('X', '1'), v('X', '2')])
    ]
  },
  {
    name: 'the_shell_select',
    steps: [
      read('shellValue', '', SHELLS),
      read('shellValue', 'zsh', SHELLS),
      read('shellValue', 'pwsh.exe', SHELLS),
      read('shellValue', 'fish', SHELLS),
      read('shellValue', '/opt/fish', SHELLS),
      read('shellValue', 'bash', null),
      read('shellOptions', '__system_default__', SHELLS, 'App default'),
      read('shellOptions', '/opt/fish', SHELLS, 'App default'),
      read('shellOptions', 'fish', null, 'System default'),
      read('shellFromValue', '__system_default__'),
      read('shellFromValue', '/bin/bash')
    ]
  }
]

import type { Surface } from '../types'

/** SSH lives only in the desktop app, and the seeded world has no SSH host to connect to. */
const DESKTOP_ONLY =
  'the SSH section renders only in the desktop app, and the seeded world has no SSH host or saved profile'
const NO_SESSION =
  'needs a connected SSH session in the desktop app; the seeded world has no SSH host'

const ssh = (id: string, title: string, file: string, blocked = NO_SESSION): Surface => ({
  id: `ssh.${id}`,
  area: 'ssh',
  title,
  source: { overlay: [`components/ssh/${file}`] },
  reference: 'tauri',
  blocked,
  steps: []
})

export const SSH: Surface[] = [
  ssh('panel-menu', 'SSH section menu', 'SSHPanel.tsx', DESKTOP_ONLY),
  ssh('panel-context-menu', 'SSH section context menu', 'SSHPanel.tsx', DESKTOP_ONLY),
  ssh('host-editor', 'SSH host editor', 'SSHHostEditor.tsx', DESKTOP_ONLY),
  ssh('host-menu', 'SSH host row menu', 'SSHHostRow.tsx', DESKTOP_ONLY),
  ssh('host-context-menu', 'SSH host row context menu', 'SSHHostRow.tsx', DESKTOP_ONLY),
  ssh('delete-host', 'Delete SSH host confirm', 'SSHPanelDialogs.tsx', DESKTOP_ONLY),
  ssh('close-session', 'Close SSH session confirm', 'SSHWorkspace.tsx'),
  ssh('file-editor', 'Remote file editor', 'SSHFileEditor.tsx'),
  ssh('new-entry', 'Remote file explorer new file dialog', 'SSHFileExplorer.tsx'),
  ssh('delete-entry', 'Remote file explorer delete confirm', 'SSHFileExplorer.tsx'),
  ssh('replace-files', 'Remote file explorer replace files confirm', 'SSHFileExplorer.tsx')
]

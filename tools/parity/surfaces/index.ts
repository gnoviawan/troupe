import type { Surface } from '../types'
import { AUTOMATIONS } from './automations'
import { CHAT } from './chat'
import { CHROME } from './chrome'
import { COMPOSER } from './composer'
import { EDITOR } from './editor'
import { EXPLORER } from './explorer'
import { GIT } from './git'
import { GITHUB } from './github'
import { PHONE } from './phone'
import { RAIL } from './rail'
import { SETTINGS } from './settings'
import { SHELL } from './shell'
import { SNAPSHOTS } from './snapshots'
import { SSH } from './ssh'
import { TERMINAL } from './terminal'

/** The starter list: surfaces of today's UI that the GPUIX matrix drove. R3b grows it to the full inventory. */
export const SURFACES: Surface[] = [
  ...RAIL,
  ...CHAT,
  ...COMPOSER,
  ...EXPLORER,
  ...EDITOR,
  ...GIT,
  ...TERMINAL,
  ...CHROME,
  ...SETTINGS,
  ...SSH,
  ...SNAPSHOTS,
  ...AUTOMATIONS,
  ...GITHUB,
  ...SHELL,
  ...PHONE
]

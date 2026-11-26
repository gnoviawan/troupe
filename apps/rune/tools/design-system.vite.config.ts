// The design-system dev server, plus read access to the real node_modules
// folder: a worktree that links node_modules from another checkout would
// otherwise get 403 for every font file, and Chromium would draw a fallback face.
import { realpathSync } from 'node:fs'
import { join } from 'node:path'
import { mergeConfig } from 'vite'
import designSystem from '../../../design-system/vite.config'

const ROOT = join(__dirname, '..', '..', '..')

export default mergeConfig(designSystem, {
  server: { fs: { allow: [ROOT, realpathSync(join(ROOT, 'node_modules'))] } }
})

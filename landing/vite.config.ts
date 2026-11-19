import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import reactSsg from 'vite-plugin-react-ssg'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

/**
 * Puts the stable version from the release snapshot into the JSON-LD
 * `softwareVersion` (placeholder `%TROUPE_STABLE_VERSION%` in index.html),
 * so the structured data never goes stale.
 */
function releaseVersionPlugin(): Plugin {
  return {
    name: 'troupe-release-version',
    transformIndexHtml(html) {
      const snapshotFile = path.resolve(__dirname, 'src/data/releases.snapshot.json')
      const snapshot = JSON.parse(readFileSync(snapshotFile, 'utf8')) as { stable?: { version?: string } | null }
      const version = snapshot.stable?.version ?? '0.0.0'
      return html.replaceAll('%TROUPE_STABLE_VERSION%', version)
    },
  }
}

/**
 * Local dev and preview have no Pages Functions. Read the public testimonial
 * list and avatars from production so the section looks real locally.
 * GET only: a form POST in dev must never reach the live database.
 */
const liveTestimonialsProxy = {
  '/api/testimonials': {
    target: 'https://usetroupe.dev',
    changeOrigin: true,
    bypass: (req: { method?: string; url?: string }) => (req.method === 'GET' ? undefined : req.url),
  },
}

// https://vite.dev/config/
export default defineConfig({
  server: { proxy: liveTestimonialsProxy },
  preview: { proxy: liveTestimonialsProxy },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  plugins: [
    react(),
    tailwindcss(),
    reactSsg(),
    releaseVersionPlugin(),
  ],
})
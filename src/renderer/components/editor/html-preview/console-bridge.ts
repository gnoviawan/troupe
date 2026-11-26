/** Tags every message the injected bridge posts to the editor. */
export const BRIDGE_SOURCE = 'troupe-html-preview'

export const CONSOLE_LEVELS = ['log', 'info', 'warn', 'error'] as const

export type ConsoleLevel = (typeof CONSOLE_LEVELS)[number]

export interface ConsoleEntry {
  level: ConsoleLevel
  text: string
}

/** Longest message text kept; a page can log megabytes in one call. */
const MAX_TEXT = 2000

/**
 * Runs first in the previewed page: forwards console calls, uncaught errors
 * and unhandled rejections to the editor. The page is sandboxed without
 * `allow-same-origin`, so `postMessage` is the only way out.
 */
const BRIDGE_SCRIPT = `<script>(()=>{const S=${JSON.stringify(BRIDGE_SOURCE)};const fmt=(a)=>a.map((v)=>{if(typeof v==='string')return v;if(v instanceof Error)return v.stack||String(v);try{return JSON.stringify(v)}catch{return String(v)}}).join(' ');const post=(level,text)=>{try{parent.postMessage({source:S,level,text:String(text).slice(0,${MAX_TEXT})},'*')}catch{}};for(const level of ${JSON.stringify(CONSOLE_LEVELS)}){const orig=console[level];console[level]=(...a)=>{post(level,fmt(a));return orig.apply(console,a)}}addEventListener('error',(e)=>post('error',e.message||'Script error'));addEventListener('unhandledrejection',(e)=>post('error','Unhandled rejection: '+fmt([e.reason])))})()</script>`

/**
 * Comments and raw-text elements are matched whole, so a `<head>` written
 * inside a script string or a comment is never taken for the real one.
 */
const HEAD_SCAN =
  /<!--[\s\S]*?-->|<(script|style|textarea|title|template|noscript|xmp)\b[\s\S]*?<\/\1\s*>|<head(?:\s[^>]*)?>/gi
const DOCTYPE = /^\s*<!doctype[^>]*>/i

function findHeadOpen(html: string): RegExpExecArray | null {
  HEAD_SCAN.lastIndex = 0
  for (let match = HEAD_SCAN.exec(html); match; match = HEAD_SCAN.exec(html)) {
    if (/^<head/i.test(match[0])) return match
  }
  return null
}

/**
 * `html` with the console bridge right after `<head>`. Without a head it goes
 * after the doctype, so the page keeps standards mode, or else at the start.
 */
export function injectConsoleBridge(html: string): string {
  const anchor = findHeadOpen(html) ?? DOCTYPE.exec(html)
  if (!anchor) return BRIDGE_SCRIPT + html
  const end = anchor.index + anchor[0].length
  return html.slice(0, end) + BRIDGE_SCRIPT + html.slice(end)
}

/** A bridge message, or `null` for anything else a page may post. */
export function parseBridgeMessage(data: unknown): ConsoleEntry | null {
  if (typeof data !== 'object' || data === null) return null
  const { source, level, text } = data as Record<string, unknown>
  if (source !== BRIDGE_SOURCE || typeof text !== 'string') return null
  if (!CONSOLE_LEVELS.includes(level as ConsoleLevel)) return null
  return { level: level as ConsoleLevel, text: text.slice(0, MAX_TEXT) }
}

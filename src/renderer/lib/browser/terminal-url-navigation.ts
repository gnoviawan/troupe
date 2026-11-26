import { openLink } from './open-link'

/** Open a URL a user Ctrl/Cmd-clicked in terminal output, where "Open links in" says. */
export async function openTerminalUrl(url: string): Promise<void> {
  await openLink(url)
}

import { Terminal } from '@xterm/xterm'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  attachCommandStatusGutter,
  commandStatusFromExitCode,
  commandStatusLabel
} from './command-status-gutter'

function stubCanvasContext(): void {
  const context = new Proxy(
    { measureText: () => ({ width: 8 }) },
    {
      get: (target, prop) =>
        prop in target ? (target as Record<string | symbol, unknown>)[prop] : () => undefined,
      set: () => true
    }
  )
  HTMLCanvasElement.prototype.getContext = (() =>
    context) as unknown as HTMLCanvasElement['getContext']
}

const PROMPT = '\x1b]133;A;termul\x07'
const finish = (code: number): string => `\x1b]133;D;${code};termul\x07`

describe('command-status-gutter', () => {
  const originalGetContext = HTMLCanvasElement.prototype.getContext
  const terminals: Terminal[] = []
  const containers: HTMLDivElement[] = []

  function setup(cols = 80) {
    stubCanvasContext()
    const container = document.createElement('div')
    document.body.appendChild(container)
    const terminal = new Terminal({ cols, rows: 24, scrollback: 1000, allowProposedApi: true })
    terminal.open(container)
    terminals.push(terminal)
    containers.push(container)
    const decorations = vi.spyOn(terminal, 'registerDecoration')
    attachCommandStatusGutter(terminal)
    const write = (data: string): Promise<void> =>
      new Promise((resolve) => {
        terminal.write(data, resolve)
      })
    return { terminal, decorations, write }
  }

  afterEach(() => {
    for (const t of terminals.splice(0)) t.dispose()
    for (const c of containers.splice(0)) c.remove()
    HTMLCanvasElement.prototype.getContext = originalGetContext
    vi.restoreAllMocks()
  })

  it('maps exit codes to status and label', () => {
    expect(commandStatusFromExitCode(0)).toBe('success')
    expect(commandStatusFromExitCode(1)).toBe('failure')
    expect(commandStatusFromExitCode(130)).toBe('failure')
    expect(commandStatusLabel(0)).toBe('Command succeeded')
    expect(commandStatusLabel(127)).toBe('Command failed (exit code 127)')
  })

  it('places a dot on the command row of the first prompt', async () => {
    const { decorations, write } = setup()
    await write(`${PROMPT}ls\r\nfile.txt\r\n${finish(0)}${PROMPT}`)

    expect(decorations).toHaveBeenCalledTimes(1)
    const options = decorations.mock.calls[0][0]
    expect(options.marker.line).toBe(0)
    expect(options.x).toBe(0)
    expect(options.width).toBe(1)
  })

  it('skips the spacer rows the host injects around the prompt mark', async () => {
    const { decorations, write } = setup()
    // Layout produced by the host prompt spacer: blank row, mark, blank row, echo.
    await write(`out\r\n\r\n${PROMPT}\r\nfalse\r\n${finish(1)}`)

    expect(decorations).toHaveBeenCalledTimes(1)
    expect(decorations.mock.calls[0][0].marker.line).toBe(3)
  })

  it('renders success and failure classes with an accessible label', async () => {
    const { decorations, write } = setup()
    const renderers: Array<(element: HTMLElement) => void> = []
    decorations.mockImplementation(
      () =>
        ({
          onRender: (listener: (element: HTMLElement) => void) => {
            renderers.push(listener)
            return { dispose: () => undefined }
          },
          dispose: () => undefined
        }) as unknown as ReturnType<Terminal['registerDecoration']>
    )
    await write(`${PROMPT}true\r\n${finish(0)}${PROMPT}false\r\n${finish(2)}${PROMPT}`)

    expect(renderers).toHaveLength(2)
    const [ok, failed] = renderers.map((render) => {
      const element = document.createElement('div')
      render(element)
      return element
    })
    expect(ok.classList.contains('termul-command-status--success')).toBe(true)
    expect(ok.getAttribute('aria-label')).toBe('Command succeeded')
    expect(failed.classList.contains('termul-command-status--failure')).toBe(true)
    expect(failed.title).toBe('Command failed (exit code 2)')
  })

  it('draws nothing for a prompt with no finished command', async () => {
    const { decorations, write } = setup()
    // Empty Enter: two prompts, no finish mark.
    await write(`${PROMPT}\r\n${PROMPT}`)
    expect(decorations).not.toHaveBeenCalled()
  })

  it('closes a prompt only once', async () => {
    const { decorations, write } = setup()
    await write(`${PROMPT}ls\r\n${finish(0)}${finish(1)}`)
    expect(decorations).toHaveBeenCalledTimes(1)
  })

  it('ignores plain 133 marks from user prompt frameworks', async () => {
    const { decorations, write } = setup()
    await write('\x1b]133;A\x07ls\r\n\x1b]133;D;1\x07\x1b]133;A\x07')
    expect(decorations).not.toHaveBeenCalled()
  })

  it('ignores marks in the alternate screen', async () => {
    const { decorations, write } = setup()
    await write(`\x1b[?1049h${PROMPT}vim\r\n${finish(0)}\x1b[?1049l`)
    expect(decorations).not.toHaveBeenCalled()
  })

  it('handles a finish mark split across writes', async () => {
    const { decorations, write } = setup()
    await write(`${PROMPT}make\r\nerror\r\n\x1b]133;D;`)
    await write('2;termul\x07')
    expect(decorations).toHaveBeenCalledTimes(1)
    expect(decorations.mock.calls[0][0].marker.line).toBe(0)
  })

  it('anchors to the command row after the prompt scrolled into scrollback', async () => {
    const { decorations, write } = setup()
    const output = Array.from({ length: 40 }, (_, i) => `line ${i}`).join('\r\n')
    await write(`${PROMPT}seq 40\r\n${output}\r\n${finish(0)}`)
    expect(decorations).toHaveBeenCalledTimes(1)
    expect(decorations.mock.calls[0][0].marker.line).toBe(0)
  })

  it('is idempotent per terminal', async () => {
    const { terminal, decorations, write } = setup()
    attachCommandStatusGutter(terminal)
    await write(`${PROMPT}ls\r\n${finish(0)}`)
    expect(decorations).toHaveBeenCalledTimes(1)
  })
})

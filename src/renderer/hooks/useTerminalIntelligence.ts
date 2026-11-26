/**
 * Terminal Intelligence Hook
 *
 * This hook integrates terminal intelligence with active terminals,
 * capturing command execution and feeding data to the analysis pipeline.
 */

import { useEffect, useCallback, useRef } from 'react'
import { useTerminalStore } from '@/stores/terminal-store'
import { useTerminalIntelligenceStore } from '@/stores/terminal-intelligence-store'
import { useProjectStore } from '@/stores/project-store'
import { invokeIpc } from '@/lib/ipc/tauri'
import { randomUUID } from '@/lib/uuid'
import type {
  CommandAnalysis,
  CommandSignature
} from '@/types/terminal-intelligence.types'

/** Hook to automatically capture and analyze terminal commands. */
export function useTerminalIntelligence() {
  const { terminals, peekTranscript, consumeTranscript } = useTerminalStore()
  const { addCommandToHistory, updateCommand } = useTerminalIntelligenceStore()
  const activeProjectId = useProjectStore((s) => s.activeProjectId)
  const lastTranscriptRef = useRef<Map<string, string>>(new Map())
  const commandStartTimeRef = useRef<Map<string, number>>(new Map())
  const capturedCommandsRef = useRef<Set<string>>(new Set())

  const analyzeCommand = useCallback(
    async (
      command: string,
      terminalId: string,
      output: string,
      duration: number,
      succeeded: boolean,
      exitCode?: number
    ) => {
      if (!activeProjectId) return

      try {
        // Analyze via Tauri backend
        const result = await invokeIpc<{
          signature: CommandSignature
          succeeded: boolean
        }>('analyze_command', {
          command,
          output: output.slice(0, 5000), // Limit output size
          terminalId,
          projectId: activeProjectId
        })

        if (!result.success) {
          console.warn('Failed to analyze command:', result.error)
          return
        }

        // Create command analysis record
        const analysis: CommandAnalysis = {
          id: randomUUID(),
          terminalId,
          projectId: activeProjectId,
          command,
          signature: result.data.signature,
          exitCode,
          duration,
          timestamp: Date.now(),
          output,
          outputSummary: extractOutputSummary(output),
          succeeded,
          tags: extractTags(result.data.signature, succeeded),
          relatedCommands: [],
          context: {
            cwd: undefined
          }
        }

        // Add to store for persistence and search
        addCommandToHistory(analysis)
      } catch (error) {
        console.error('Error analyzing command:', error)
      }
    },
    [activeProjectId, addCommandToHistory]
  )

  // Monitor terminal transcripts for new commands
  useEffect(() => {
    const interval = setInterval(() => {
      terminals.forEach((terminal) => {
        const transcript = peekTranscript(terminal.ptyId || '')
        const lastTranscript = lastTranscriptRef.current.get(terminal.ptyId || '') || ''

        if (transcript && transcript !== lastTranscript) {
          lastTranscriptRef.current.set(terminal.ptyId || '', transcript)

          // Extract new content
          const newContent = transcript.slice(lastTranscript.length)
          analyzeNewContent(newContent, terminal.id, terminal.ptyId || '')
        }
      })
    }, 1000)

    return () => clearInterval(interval)
  }, [terminals, peekTranscript, analyzeCommand])

  const analyzeNewContent = useCallback(
    (content: string, terminalId: string, ptyId: string) => {
      // Look for command patterns (e.g., "$ command" or "# command")
      const commandPattern = /[\$#]\s+(.+?)(?:\n|$)/g
      let match

      while ((match = commandPattern.exec(content)) !== null) {
        const command = match[1].trim()
        if (!command || capturedCommandsRef.current.has(command)) continue

        capturedCommandsRef.current.add(command)

        // Look for exit code on next line
        const exitCodePattern = /(exit code|exit status|status code)[:\s]+(\d+)/i
        const exitCodeMatch = exitCodePattern.exec(content.slice(match.index))
        const exitCode = exitCodeMatch ? parseInt(exitCodeMatch[2], 10) : undefined

        // Estimate duration from timestamp if available
        const startTime = commandStartTimeRef.current.get(ptyId) || Date.now()
        const duration = Date.now() - startTime

        // Determine if succeeded (default to true if no exit code)
        const succeeded = exitCode === undefined || exitCode === 0

        // Get relevant output (up to 10000 chars after command)
        const outputEnd = match.index + 10000
        const output = content.slice(match.index, outputEnd)

        analyzeCommand(
          command,
          terminalId,
          output,
          duration,
          succeeded,
          exitCode
        )

        commandStartTimeRef.current.set(ptyId, Date.now())
      }
    },
    [analyzeCommand]
  )

  return {
    analyzeCommand
  }
}

/** Extract summary from command output. */
function extractOutputSummary(output: string): string | undefined {
  const lines = output.split('\n').filter((line) => line.trim())

  // Look for error/warning lines
  for (const line of lines) {
    const lower = line.toLowerCase()
    if (lower.includes('error') || lower.includes('failed') || lower.includes('failed')) {
      return line.trim()
    }
  }

  // Return last line if it's meaningful
  if (lines.length > 0) {
    const lastLine = lines[lines.length - 1].trim()
    if (lastLine.length > 10 && lastLine.length < 200) {
      return lastLine
    }
  }

  return undefined
}

/** Extract tags from command and success status. */
function extractTags(signature: CommandSignature, succeeded: boolean): string[] {
  const tags = [signature.category]

  if (!succeeded) {
    tags.push('failed')
  }

  if (signature.semanticTokens.some((t) => t.includes('slow'))) {
    tags.push('slow')
  }

  return tags
}

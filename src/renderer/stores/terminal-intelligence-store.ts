/**
 * Terminal Intelligence Store
 *
 * Zustand store for managing command history, semantic analysis, patterns,
 * and AI-powered suggestions across all terminals in a project.
 */

import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'
import type {
  CommandAnalysis,
  CommandPattern,
  CommandSuggestion,
  TerminalIntelligenceConfig,
  TerminalIntelligenceStats,
  SearchCommandsQuery,
  SearchCommandsResult
} from '@/types/terminal-intelligence.types'

export interface TerminalIntelligenceState {
  // State
  commandHistory: CommandAnalysis[]
  patterns: CommandPattern[]
  suggestions: Map<string, CommandSuggestion[]>
  stats: Map<string, TerminalIntelligenceStats>
  config: TerminalIntelligenceConfig
  isAnalyzing: boolean
  lastAnalyzedAt: number

  // Actions
  addCommandToHistory: (command: CommandAnalysis) => void
  updateCommand: (id: string, updates: Partial<CommandAnalysis>) => void
  removeCommand: (id: string) => void
  clearHistory: (projectId?: string) => void

  getCommandsForTerminal: (terminalId: string) => CommandAnalysis[]
  getCommandsForProject: (projectId: string) => CommandAnalysis[]

  setPatterns: (patterns: CommandPattern[]) => void
  addPattern: (pattern: CommandPattern) => void

  setSuggestions: (commandId: string, suggestions: CommandSuggestion[]) => void
  getSuggestions: (commandId: string) => CommandSuggestion[]

  setStats: (projectId: string, stats: TerminalIntelligenceStats) => void
  getStats: (projectId: string) => TerminalIntelligenceStats | undefined

  updateConfig: (config: Partial<TerminalIntelligenceConfig>) => void

  setAnalyzing: (analyzing: boolean) => void
  updateLastAnalyzedAt: () => void

  reset: () => void
}

const DEFAULT_CONFIG: TerminalIntelligenceConfig = {
  enableSemanticSearch: true,
  enableSuggestions: true,
  enablePatternDetection: true,
  enableOutputAnalysis: true,
  maxHistorySize: 10000,
  suggestionConfidenceThreshold: 0.6,
  useAIAnalysis: true
}

export const useTerminalIntelligenceStore = create<TerminalIntelligenceState>()(
  devtools(
    persist(
      (set, get) => ({
        commandHistory: [],
        patterns: [],
        suggestions: new Map(),
        stats: new Map(),
        config: DEFAULT_CONFIG,
        isAnalyzing: false,
        lastAnalyzedAt: 0,

        addCommandToHistory: (command: CommandAnalysis): void => {
          set((state) => {
            const history = [...state.commandHistory, command]
            // Trim to max size, keeping most recent
            if (history.length > state.config.maxHistorySize) {
              history.splice(0, history.length - state.config.maxHistorySize)
            }
            return { commandHistory: history }
          })
        },

        updateCommand: (id: string, updates: Partial<CommandAnalysis>): void => {
          set((state) => ({
            commandHistory: state.commandHistory.map((cmd) =>
              cmd.id === id ? { ...cmd, ...updates } : cmd
            )
          }))
        },

        removeCommand: (id: string): void => {
          set((state) => ({
            commandHistory: state.commandHistory.filter((cmd) => cmd.id !== id)
          }))
        },

        clearHistory: (projectId?: string): void => {
          set((state) => ({
            commandHistory: projectId
              ? state.commandHistory.filter((cmd) => cmd.projectId !== projectId)
              : []
          }))
        },

        getCommandsForTerminal: (terminalId: string): CommandAnalysis[] => {
          return get().commandHistory.filter((cmd) => cmd.terminalId === terminalId)
        },

        getCommandsForProject: (projectId: string): CommandAnalysis[] => {
          return get().commandHistory.filter((cmd) => cmd.projectId === projectId)
        },

        setPatterns: (patterns: CommandPattern[]): void => {
          set({ patterns })
        },

        addPattern: (pattern: CommandPattern): void => {
          set((state) => {
            const existing = state.patterns.find((p) => p.id === pattern.id)
            if (existing) {
              return {
                patterns: state.patterns.map((p) => (p.id === pattern.id ? pattern : p))
              }
            }
            return { patterns: [...state.patterns, pattern] }
          })
        },

        setSuggestions: (commandId: string, suggestions: CommandSuggestion[]): void => {
          set((state) => {
            const newSuggestions = new Map(state.suggestions)
            newSuggestions.set(commandId, suggestions)
            return { suggestions: newSuggestions }
          })
        },

        getSuggestions: (commandId: string): CommandSuggestion[] => {
          return get().suggestions.get(commandId) ?? []
        },

        setStats: (projectId: string, stats: TerminalIntelligenceStats): void => {
          set((state) => {
            const newStats = new Map(state.stats)
            newStats.set(projectId, stats)
            return { stats: newStats }
          })
        },

        getStats: (projectId: string): TerminalIntelligenceStats | undefined => {
          return get().stats.get(projectId)
        },

        updateConfig: (config: Partial<TerminalIntelligenceConfig>): void => {
          set((state) => ({
            config: { ...state.config, ...config }
          }))
        },

        setAnalyzing: (analyzing: boolean): void => {
          set({ isAnalyzing: analyzing })
        },

        updateLastAnalyzedAt: (): void => {
          set({ lastAnalyzedAt: Date.now() })
        },

        reset: (): void => {
          set({
            commandHistory: [],
            patterns: [],
            suggestions: new Map(),
            stats: new Map(),
            config: DEFAULT_CONFIG,
            isAnalyzing: false,
            lastAnalyzedAt: 0
          })
        }
      }),
      {
        name: 'terminal-intelligence-store',
        partialize: (state) => ({
          commandHistory: state.commandHistory,
          patterns: state.patterns,
          config: state.config
        })
      }
    )
  )
)

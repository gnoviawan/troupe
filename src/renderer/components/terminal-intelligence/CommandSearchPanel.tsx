/**
 * Command Search Panel Component
 *
 * Provides semantic search, suggestions, and pattern recommendations for
 * terminal commands across all terminals in a project.
 */

import { useState, useCallback, useMemo } from 'react'
import { Search, TrendingUp, Zap, AlertCircle, CheckCircle } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/cn'
import { useTerminalIntelligenceStore } from '@/stores/terminal-intelligence-store'
import { useProjectStore } from '@/stores/project-store'
import type { CommandAnalysis, CommandSuggestion, CommandPattern } from '@/types/terminal-intelligence.types'

export interface CommandSearchPanelProps {
  terminalId?: string
  onSelectCommand?: (command: CommandAnalysis) => void
  className?: string
}

export function CommandSearchPanel({
  terminalId,
  onSelectCommand,
  className
}: CommandSearchPanelProps) {
  const [searchQuery, setSearchQuery] = useState('')
  const [isSearching, setIsSearching] = useState(false)
  const [activeTab, setActiveTab] = useState<'search' | 'patterns' | 'suggestions'>('search')

  const projectId = useProjectStore((s) => s.activeProjectId)
  const {
    commandHistory,
    patterns,
    suggestions,
    config,
    getCommandsForProject,
    getSuggestions
  } = useTerminalIntelligenceStore()

  const projectCommands = useMemo(() => {
    if (!projectId) return []
    return getCommandsForProject(projectId)
  }, [projectId, getCommandsForProject])

  // Semantic search implementation
  const searchResults = useMemo(() => {
    if (!searchQuery.trim()) return []

    const query = searchQuery.toLowerCase()
    const scored = projectCommands
      .map((cmd) => {
        let score = 0

        // Exact match in command
        if (cmd.command.toLowerCase().includes(query)) {
          score += 10
        }

        // Match in semantic tokens
        if (cmd.signature.semanticTokens.some((t) => t.includes(query))) {
          score += 5
        }

        // Match in category
        if (cmd.signature.category.includes(query)) {
          score += 3
        }

        // Match in tags
        if (cmd.tags.some((tag) => tag.includes(query))) {
          score += 2
        }

        // Match in output summary
        if (cmd.outputSummary?.toLowerCase().includes(query)) {
          score += 1
        }

        return { cmd, score }
      })
      .filter((item) => item.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 20)

    return scored.map((item) => item.cmd)
  }, [searchQuery, projectCommands])

  // Get pattern recommendations for current context
  const relevantPatterns = useMemo(() => {
    if (!terminalId) return patterns
    const terminalCommands = projectCommands.filter((c) => c.terminalId === terminalId)
    if (terminalCommands.length === 0) return []

    // Find patterns that match recent commands
    return patterns.filter((p) => {
      const recentSigs = terminalCommands
        .slice(-5)
        .map((c) => c.signature.command)
      return p.commands.some((pc) => recentSigs.includes(pc.command))
    })
  }, [terminalId, patterns, projectCommands])

  // Get suggestions for selected command
  const displayedSuggestions = useMemo(() => {
    if (searchResults.length === 0) return []
    const lastCommand = searchResults[0]
    return getSuggestions(lastCommand.id)
  }, [searchResults, getSuggestions])

  const handleSearch = useCallback(async (query: string) => {
    setSearchQuery(query)
    if (query.trim()) {
      setIsSearching(true)
      // Simulate AI analysis delay
      await new Promise((resolve) => setTimeout(resolve, 300))
      setIsSearching(false)
    }
  }, [])

  const handleSelectCommand = useCallback(
    (cmd: CommandAnalysis) => {
      onSelectCommand?.(cmd)
    },
    [onSelectCommand]
  )

  return (
    <div className={cn('flex flex-col h-full bg-surface border-l border-divider', className)}>
      {/* Header */}
      <div className="flex-shrink-0 p-4 border-b border-divider">
        <div className="flex items-center gap-2 mb-3">
          <Search className="w-4 h-4 text-muted-foreground" />
          <h2 className="font-semibold text-sm">Command Intelligence</h2>
        </div>
        <div className="relative">
          <Input
            placeholder="Search commands..."
            value={searchQuery}
            onChange={(e) => handleSearch(e.target.value)}
            className="pl-3 h-9"
          />
          {isSearching && (
            <div className="absolute right-3 top-2.5">
              <div className="w-4 h-4 border-2 border-muted-foreground border-t-foreground rounded-full animate-spin" />
            </div>
          )}
        </div>
      </div>

      {/* Tabs */}
      <div className="flex-shrink-0 flex border-b border-divider px-4">
        <button
          onClick={() => setActiveTab('search')}
          className={cn(
            'px-3 py-2 text-xs font-medium border-b-2 transition-colors',
            activeTab === 'search'
              ? 'border-brand text-foreground'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          )}
        >
          Search ({searchResults.length})
        </button>
        <button
          onClick={() => setActiveTab('patterns')}
          className={cn(
            'px-3 py-2 text-xs font-medium border-b-2 transition-colors',
            activeTab === 'patterns'
              ? 'border-brand text-foreground'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          )}
        >
          Patterns ({relevantPatterns.length})
        </button>
        <button
          onClick={() => setActiveTab('suggestions')}
          className={cn(
            'px-3 py-2 text-xs font-medium border-b-2 transition-colors',
            activeTab === 'suggestions'
              ? 'border-brand text-foreground'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          )}
        >
          Tips
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {activeTab === 'search' && (
          <SearchResultsTab results={searchResults} onSelectCommand={handleSelectCommand} />
        )}
        {activeTab === 'patterns' && (
          <PatternsTab patterns={relevantPatterns} />
        )}
        {activeTab === 'suggestions' && (
          <SuggestionsTab suggestions={displayedSuggestions} />
        )}
      </div>
    </div>
  )
}

interface SearchResultsTabProps {
  results: CommandAnalysis[]
  onSelectCommand: (cmd: CommandAnalysis) => void
}

function SearchResultsTab({ results, onSelectCommand }: SearchResultsTabProps) {
  if (results.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-6 text-center">
        <Search className="w-8 h-8 text-muted-foreground mb-2" />
        <p className="text-sm text-muted-foreground">
          Search commands by name, type, or intent
        </p>
      </div>
    )
  }

  return (
    <div className="divide-y divide-divider">
      {results.map((cmd) => (
        <div
          key={cmd.id}
          onClick={() => onSelectCommand(cmd)}
          className="p-3 cursor-pointer hover:bg-muted transition-colors group"
        >
          <div className="flex items-start justify-between gap-2 mb-2">
            <code className="text-xs font-mono text-foreground break-all flex-1">
              {cmd.command}
            </code>
            {cmd.succeeded ? (
              <CheckCircle className="w-4 h-4 text-success flex-shrink-0 mt-0.5" />
            ) : (
              <AlertCircle className="w-4 h-4 text-danger flex-shrink-0 mt-0.5" />
            )}
          </div>

          {cmd.outputSummary && (
            <p className="text-xs text-muted-foreground mb-2">{cmd.outputSummary}</p>
          )}

          <div className="flex flex-wrap gap-1">
            <Badge variant="secondary" className="text-xs">
              {cmd.signature.category}
            </Badge>
            {cmd.tags.slice(0, 2).map((tag) => (
              <Badge key={tag} variant="tertiary" className="text-xs">
                {tag}
              </Badge>
            ))}
            <span className="text-xs text-muted-foreground ml-auto">
              {cmd.duration}ms
            </span>
          </div>
        </div>
      ))}
    </div>
  )
}

interface PatternsTabProps {
  patterns: CommandPattern[]
}

function PatternsTab({ patterns }: PatternsTabProps) {
  if (patterns.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-6 text-center">
        <TrendingUp className="w-8 h-8 text-muted-foreground mb-2" />
        <p className="text-sm text-muted-foreground">
          Workflow patterns will appear here
        </p>
      </div>
    )
  }

  return (
    <div className="divide-y divide-divider">
      {patterns.map((pattern) => (
        <div key={pattern.id} className="p-3 hover:bg-muted transition-colors">
          <div className="flex items-start justify-between mb-2">
            <h4 className="text-sm font-medium">
              {pattern.name || 'Workflow Pattern'}
            </h4>
            <span className="text-xs text-muted-foreground">
              {Math.round(pattern.successRate * 100)}%
            </span>
          </div>

          <div className="text-xs text-muted-foreground mb-2 space-y-1">
            {pattern.commands.slice(0, 3).map((cmd, i) => (
              <div key={i} className="flex items-center gap-1">
                <span className="text-muted-foreground">→</span>
                <code className="text-xs font-mono">{cmd.command}</code>
              </div>
            ))}
            {pattern.commands.length > 3 && (
              <div className="text-xs text-muted-foreground">
                +{pattern.commands.length - 3} more
              </div>
            )}
          </div>

          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">Used {pattern.frequency}x</span>
            <span className="text-muted-foreground">
              {Math.round(pattern.averageDuration)}ms avg
            </span>
          </div>
        </div>
      ))}
    </div>
  )
}

interface SuggestionsTabProps {
  suggestions: CommandSuggestion[]
}

function SuggestionsTab({ suggestions }: SuggestionsTabProps) {
  if (suggestions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-6 text-center">
        <Zap className="w-8 h-8 text-muted-foreground mb-2" />
        <p className="text-sm text-muted-foreground">
          Search a command to see improvement tips
        </p>
      </div>
    )
  }

  return (
    <div className="divide-y divide-divider">
      {suggestions.map((suggestion, i) => (
        <div key={i} className="p-3 hover:bg-muted transition-colors">
          <div className="flex items-start justify-between mb-2">
            <code className="text-xs font-mono text-foreground break-all">
              {suggestion.command}
            </code>
            <span className="text-xs font-medium text-success flex-shrink-0 ml-2">
              {Math.round(suggestion.confidence * 100)}%
            </span>
          </div>

          <p className="text-xs text-muted-foreground mb-2">{suggestion.reason}</p>

          {suggestion.impact && (
            <div className="text-xs bg-muted p-2 rounded text-muted-foreground">
              💡 {suggestion.impact}
            </div>
          )}

          <Button size="sm" variant="secondary" className="mt-2 w-full text-xs">
            Copy Command
          </Button>
        </div>
      ))}
    </div>
  )
}

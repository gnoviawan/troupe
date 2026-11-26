/**
 * Terminal Intelligence System Types
 *
 * This module defines the contract for the AI-powered terminal intelligence
 * system, providing semantic command search, pattern detection, and intelligent
 * command suggestions across all terminals in a project.
 */

/** Semantic fingerprint of a command for matching and analysis. */
export interface CommandSignature {
  /** Normalized command without arguments (e.g., 'git commit', 'npm run') */
  command: string
  /** Hash of the command intent (for deduplication) */
  intentHash: string
  /** Semantic tokens for search indexing */
  semanticTokens: string[]
  /** Command category (git, build, test, dev, deploy, etc.) */
  category: 'git' | 'build' | 'test' | 'dev' | 'deploy' | 'other'
  /** Confidence score (0-1) for category classification */
  categoryConfidence: number
}

/** Analyzed result of a command execution. */
export interface CommandAnalysis {
  /** Unique identifier for this command record */
  id: string
  /** Terminal ID where command was executed */
  terminalId: string
  /** Project ID for scoping */
  projectId: string
  /** The command that was executed */
  command: string
  /** Command signature for semantic matching */
  signature: CommandSignature
  /** Exit code (0 for success) */
  exitCode: number | null
  /** Execution duration in milliseconds */
  duration: number
  /** Timestamp when command was executed */
  timestamp: number
  /** Terminal output/result */
  output: string
  /** Output summary (short description of what happened) */
  outputSummary?: string
  /** Whether command succeeded */
  succeeded: boolean
  /** Tags for categorization (e.g., ['ci', 'build', 'slow']) */
  tags: string[]
  /** Related command IDs (parent/child relationships) */
  relatedCommands: string[]
  /** Environment context (branch, cwd, etc.) */
  context: {
    branch?: string
    cwd?: string
    gitStatus?: string
  }
}

/** Pattern detected across multiple command executions. */
export interface CommandPattern {
  /** Pattern identifier */
  id: string
  /** Sequence of command signatures that form this pattern */
  commands: CommandSignature[]
  /** How often this pattern appears (count) */
  frequency: number
  /** Average success rate (0-1) */
  successRate: number
  /** Average duration in milliseconds */
  averageDuration: number
  /** Human-readable pattern name (e.g., 'build-and-deploy') */
  name?: string
  /** Pattern confidence score (0-1) */
  confidence: number
}

/** AI-suggested command alternative. */
export interface CommandSuggestion {
  /** ID of the original command this suggestion relates to */
  commandId: string
  /** The suggested command */
  command: string
  /** Reason for suggestion (e.g., 'faster', 'more efficient', 'best practice') */
  reason: string
  /** Confidence score (0-1) */
  confidence: number
  /** Impact of using this command (time saved, resources, etc.) */
  impact?: string
  /** Source of suggestion (e.g., 'ai_analysis', 'pattern_match', 'git_best_practice') */
  source: string
}

/** Request to analyze/suggest improvements for a command. */
export interface AnalyzeCommandRequest {
  /** The command to analyze */
  command: string
  /** Optional recent output for context */
  recentOutput?: string
  /** Optional terminal context */
  terminalId?: string
  /** Optional project context */
  projectId?: string
}

/** Response from command analysis. */
export interface AnalyzeCommandResponse {
  /** Parsed command signature */
  signature: CommandSignature
  /** Suggested alternatives */
  suggestions: CommandSuggestion[]
  /** Detected patterns this command could belong to */
  patterns: CommandPattern[]
  /** Whether this command is similar to recent failed commands */
  similarToFailures: boolean
  /** Error prevention tips if applicable */
  tips?: string[]
}

/** Semantic search query parameters. */
export interface SearchCommandsQuery {
  /** Search query (can be command name, description, or intent) */
  query: string
  /** Filter by command category */
  category?: 'git' | 'build' | 'test' | 'dev' | 'deploy' | 'other'
  /** Filter by success/failure */
  status?: 'success' | 'failure'
  /** Filter by project ID */
  projectId?: string
  /** Maximum results to return */
  limit?: number
  /** Include detailed output in results */
  includeOutput?: boolean
}

/** Result of a semantic command search. */
export interface SearchCommandsResult {
  /** Matched commands */
  commands: CommandAnalysis[]
  /** Relevance scores for each command (0-1) */
  relevanceScores: number[]
  /** Total matches available */
  totalMatches: number
  /** Query execution time in ms */
  executionTime: number
}

/** Terminal intelligence analytics snapshot. */
export interface TerminalIntelligenceStats {
  /** Project ID */
  projectId: string
  /** Total commands analyzed */
  totalCommands: number
  /** Average command success rate (0-1) */
  successRate: number
  /** Most used commands */
  topCommands: { command: string; count: number }[]
  /** Detected workflow patterns */
  patterns: CommandPattern[]
  /** Most common failure reasons */
  commonFailures: { command: string; errorPattern: string; count: number }[]
  /** Performance metrics */
  performanceInsights: {
    slowestCommands: { command: string; avgDuration: number }[]
    fastestCommands: { command: string; avgDuration: number }[]
  }
  /** Time period covered (from, to) */
  period: { from: number; to: number }
}

/** Configuration for terminal intelligence system. */
export interface TerminalIntelligenceConfig {
  /** Enable semantic search */
  enableSemanticSearch: boolean
  /** Enable command suggestions */
  enableSuggestions: boolean
  /** Enable pattern detection */
  enablePatternDetection: boolean
  /** Enable output analysis */
  enableOutputAnalysis: boolean
  /** Maximum command history to keep */
  maxHistorySize: number
  /** Minimum confidence threshold for suggestions (0-1) */
  suggestionConfidenceThreshold: number
  /** Use AI agents for analysis (requires ACP agent) */
  useAIAnalysis: boolean
  /** Preferred AI agent for analysis */
  preferredAgent?: string
}

/** IPC message types for terminal intelligence. */
export const TERMINAL_INTELLIGENCE_IPC = {
  // Analysis
  ANALYZE_COMMAND: 'analyze_command',
  GET_COMMAND_ANALYSIS: 'get_command_analysis',

  // Search
  SEARCH_COMMANDS: 'search_commands',
  GET_COMMAND_SUGGESTIONS: 'get_command_suggestions',

  // Patterns
  GET_PATTERNS: 'get_patterns',
  GET_PATTERN_DETAILS: 'get_pattern_details',

  // History
  GET_COMMAND_HISTORY: 'get_command_history',
  GET_TERMINAL_HISTORY: 'get_terminal_history',

  // Analytics
  GET_ANALYTICS: 'get_analytics',
  GET_PERFORMANCE_INSIGHTS: 'get_performance_insights',

  // Store command
  STORE_COMMAND_EXECUTION: 'store_command_execution'
} as const

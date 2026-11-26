# Terminal Intelligence System

The Terminal Intelligence System is an advanced feature in Troupe that uses semantic analysis and AI to understand, optimize, and suggest improvements for terminal workflows.

## Overview

Terminal Intelligence provides developers with:

- **Semantic Command Search**: Find commands using natural language across all projects and terminals
- **Pattern Recognition**: Automatically detect recurring workflow patterns  
- **AI Suggestions**: Get intelligent recommendations for command optimization
- **Performance Analytics**: Understand command execution patterns and performance trends
- **Output Analysis**: Automatic summarization and analysis of command results

## Architecture

### Components

```
Terminal Intelligence System
├── Renderer (TypeScript/React)
│   ├── Terminal Intelligence Store (Zustand)
│   ├── Command Search Panel Component
│   ├── useTerminalIntelligence Hook
│   └── UI Components
├── Shared Types
│   └── terminal-intelligence.types.ts
└── Rust Backend
    └── CommandAnalyzer Service
        ├── Command Signature Extraction
        ├── Pattern Detection
        ├── Output Summarization
        └── Performance Metrics
```

### Data Flow

```
Terminal Output
    ↓
useTerminalIntelligence Hook
    ↓
CommandAnalyzer (Rust)
    ├─ Extract Signature
    ├─ Analyze Output
    └─ Detect Patterns
    ↓
Terminal Intelligence Store
    ├─ Command History
    ├─ Patterns
    └─ Suggestions
    ↓
UI Components
    ├─ Command Search
    ├─ Pattern Recommendations
    └─ Performance Insights
```

## Features

### 1. Semantic Command Search

Search across command history using natural language:

```typescript
// Search results are ranked by relevance
const results = useTerminalIntelligenceStore()
  .getCommandsForProject(projectId)
  .filter(cmd => cmd.signature.semanticTokens.some(t => t.includes('deploy')))
```

Supported search dimensions:
- Command name and flags
- Semantic intent (e.g., "deploy to production")
- Command output content
- Tags and categories
- Success/failure status

### 2. Pattern Detection

Automatically identifies recurring command sequences:

```typescript
interface CommandPattern {
  commands: CommandSignature[]      // Sequence of commands
  frequency: number                  // Times this pattern was used
  successRate: number               // Success rate (0-1)
  averageDuration: number          // Average execution time
}
```

Patterns help identify:
- Build-then-deploy workflows
- Testing-then-commit sequences
- Development iteration patterns
- Error recovery patterns

### 3. AI Suggestions

For each command, receive improvement suggestions:

```typescript
interface CommandSuggestion {
  command: string                   // Suggested alternative
  reason: string                    // Why this is better
  confidence: number               // Confidence (0-1)
  impact?: string                  // Expected improvement
  source: string                   // 'ai_analysis' | 'pattern_match' | etc.
}
```

Suggestions are based on:
- Faster alternatives (e.g., parallel builds vs sequential)
- Best practices (Git conventions, npm standards)
- Previous errors (avoid known failure patterns)
- Project-specific optimization patterns

### 4. Analytics & Insights

Understand your command patterns:

```typescript
interface TerminalIntelligenceStats {
  totalCommands: number
  successRate: number
  topCommands: CommandUsage[]
  commonFailures: FailurePattern[]
  performanceInsights: PerformanceMetrics
}
```

Dashboard includes:
- Most used commands
- Success/failure rates by command type
- Performance trends
- Build time analysis
- Error pattern identification

### 5. Output Analysis

Automatic summarization of command results:

- **Error Detection**: Identifies and highlights error messages
- **Summary Generation**: Creates concise description of output
- **Pattern Matching**: Recognizes common error patterns
- **Insight Extraction**: Pulls actionable information from output

## Usage

### Enable in Terminal

Add the hook to your terminal component:

```typescript
import { useTerminalIntelligence } from '@/hooks/useTerminalIntelligence'

export function TerminalPane() {
  const { analyzeCommand } = useTerminalIntelligence()

  // Commands are automatically captured and analyzed
  // as terminal output flows in
  
  return (
    <div>
      <Terminal />
      <CommandSearchPanel terminalId={activeTerminalId} />
    </div>
  )
}
```

### Search for Commands

Use the Command Search panel to find and learn from past executions:

```typescript
// Search results update in real-time
<CommandSearchPanel 
  onSelectCommand={(cmd) => {
    // View full output
    // See suggestions
    // Find related commands
  }}
/>
```

### View Patterns

Understand your workflow:

```typescript
const patterns = useTerminalIntelligenceStore(s => s.patterns)

patterns.forEach(pattern => {
  console.log(`Pattern: ${pattern.name}`)
  console.log(`Used ${pattern.frequency} times with ${Math.round(pattern.successRate * 100)}% success`)
})
```

### Configure

Customize Terminal Intelligence behavior:

```typescript
const { updateConfig } = useTerminalIntelligenceStore()

updateConfig({
  enableSemanticSearch: true,
  enableSuggestions: true,
  enablePatternDetection: true,
  useAIAnalysis: true,
  suggestionConfidenceThreshold: 0.75,
  maxHistorySize: 20000
})
```

## Implementation Details

### Command Signature Extraction

Commands are normalized and fingerprinted:

```
Input:  "git commit -m 'Fix: add feature' --no-verify"
↓
Normalize: "git commit"
↓
Semantic Tokens: ["git", "commit"]
↓
Category: "git" (confidence: 0.95)
↓
Intent Hash: "a1b2c3d4"
```

### Pattern Detection Algorithm

Uses sliding window approach:

1. Scan command history for sequences of 2-5 commands
2. Count occurrences of each unique sequence
3. Calculate statistics (success rate, duration, frequency)
4. Filter patterns with confidence > threshold
5. Sort by frequency and relevance

### Output Summarization

Intelligent extraction of key information:

1. Split output into lines
2. Look for error/warning patterns
3. Extract meaningful last line
4. Detect common patterns (exit codes, file changes)
5. Generate concise summary (max 200 chars)

## Performance Considerations

### Storage

- **Command History**: Limited to `maxHistorySize` (default: 10,000)
- **Pattern Cache**: Recomputed on demand, cached for session
- **Store Persistence**: IndexedDB (desktop) or localStorage (web)

### Processing

- **Analysis**: Async, non-blocking per-terminal
- **Search**: O(n) but optimized with indexing
- **Pattern Detection**: O(n²) but limited window size
- **Rust Backend**: Native performance for large datasets

### Optimization Tips

```typescript
// Reduce storage
updateConfig({ maxHistorySize: 5000 })

// Disable AI for performance
updateConfig({ useAIAnalysis: false })

// Raise confidence threshold to reduce suggestions
updateConfig({ suggestionConfidenceThreshold: 0.85 })
```

## AI Integration

Terminal Intelligence integrates with Troupe's ACP agent system:

- **Command Generation**: Suggest commands using LLM
- **Output Analysis**: Understand complex error messages
- **Pattern Naming**: Generate human-readable pattern names
- **Context Awareness**: Leverage project context for suggestions

Requires a configured ACP agent (e.g., Claude, Gemini).

## Privacy & Security

- **Local Analysis**: Semantic analysis happens on-device
- **Command Filtering**: User can exclude sensitive commands
- **Output Limitation**: Only recent terminal output analyzed
- **No External Logging**: Command data never leaves device (unless AI analysis enabled)

## Troubleshooting

### No commands appearing in search

1. Ensure terminals have executed commands
2. Check `useTerminalIntelligence` hook is active
3. Verify project has an active terminal

### Patterns not detected

1. Increase `maxHistorySize` for more history
2. Run command sequences multiple times (minimum 2 occurrences)
3. Check pattern confidence threshold

### Suggestions not showing

1. Verify `enableSuggestions` is true
2. Ensure AI analysis is enabled (if using LLM suggestions)
3. Check suggestion confidence threshold isn't too high

## Future Enhancements

- 🔄 Workflow automation (record and replay patterns)
- 📊 Cross-project analytics dashboard
- 🤖 Multi-agent coordination for complex workflows
- 💾 Cloud sync for command history
- 📱 Mobile command access
- 🔗 Integration with CI/CD systems
- 🎯 Personalized performance optimization

## References

- [ADR-005: Terminal Intelligence System](../docs/adr/)
- [Architecture Guide](../docs/architecture.md)
- [ACP Agent Integration](../docs/agents.md)

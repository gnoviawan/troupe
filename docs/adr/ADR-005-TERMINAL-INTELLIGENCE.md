# ADR-005: Terminal Intelligence System

**Status**: Accepted (SUPER SAKTI Implementation)  
**Decided**: 2026-11-26  
**Author**: Claude AI  
**Decision**: Implement comprehensive AI-powered terminal command analysis system with semantic search, pattern detection, and intelligent suggestions

## Problem

Developers working with Troupe across multiple terminals and projects lose valuable context about their command workflows:

1. **Information Scatter**: Command history is fragmented across terminal buffers
2. **No Learning**: Patterns in command sequences go undetected
3. **Repeated Mistakes**: Failed commands are repeated without learning
4. **Workflow Opacity**: No visibility into how commands are really used
5. **Poor Discovery**: Can't easily find or reuse past successful commands
6. **Performance Blind Spot**: No insight into slow commands or optimization opportunities

## Proposed Solution

Implement a **Terminal Intelligence System** that:
- Captures and analyzes all terminal commands across projects
- Detects recurring workflow patterns
- Provides AI-powered suggestions for optimization
- Enables semantic search across command history
- Surfaces performance insights and analytics

## Design Principles

### 1. **Transparency by Default**
- All features work with or without AI backend
- Users understand what data is collected and how it's used
- Clear privacy boundaries

### 2. **Semantic-First Architecture**
- Commands understood by intent, not just string matching
- Semantic tokens enable flexible search and categorization
- Pattern detection based on command relationships

### 3. **Non-Intrusive Integration**
- Works alongside existing terminal features
- No impact on terminal performance
- Async analysis doesn't block user interaction

### 4. **Multi-Surface Support**
- Consistent experience across Desktop, Web, and Mobile
- Server-backed storage for cross-device access
- Fallback to local analysis if backend unavailable

### 5. **Progressive Enhancement**
- Core features work with local analysis only
- AI suggestions enhance but don't require external services
- Graceful degradation on network issues

## Architecture

### Component Layers

```
Presentation Layer (React)
├── CommandSearchPanel (search, suggestions, patterns)
├── TerminalIntelligencePanel (analytics dashboard)
└── Integration hooks (useTerminalIntelligence)
        ↓
Application Layer (Zustand Store)
├── Terminal Intelligence Store
├── Command History Management
└── Pattern & Suggestion Cache
        ↓
Service Layer (Rust + Async IPC)
├── CommandAnalyzer (signature, categorization)
├── PatternDetector (sequence analysis)
├── OutputParser (summarization)
└── PerformanceAnalytics
        ↓
Storage Layer
├── Local: IndexedDB (web) / SQLite (desktop)
├── Cache: In-memory HashMap
└── Persistence: Zustand persist middleware
```

### Data Types

**CommandSignature** (Fingerprint)
```
command: "git commit"              // Normalized form
intentHash: "a1b2c3d4"            // Deduplication key
semanticTokens: ["git", "commit"] // Search index
category: "git"                   // Classification
categoryConfidence: 0.95          // Confidence score
```

**CommandAnalysis** (Execution Record)
```
id: UUID
terminalId: string
projectId: string
command: string
signature: CommandSignature
exitCode: number | null
duration: number (ms)
timestamp: number (ms)
output: string (limited)
outputSummary: string | null
succeeded: boolean
tags: string[]
relatedCommands: string[]
context: { branch?, cwd?, gitStatus? }
```

**CommandPattern** (Recurring Sequence)
```
id: UUID
commands: CommandSignature[]      // 2-5 commands
frequency: number                 // Occurrences
successRate: number (0-1)        // Success ratio
averageDuration: number (ms)     // Typical execution time
name: string | null              // Human label
confidence: number (0-1)         // Pattern strength
```

### Analysis Pipeline

```
Terminal Output Stream
    ↓
[useTerminalIntelligence Hook]
    - Monitors transcript changes
    - Extracts command patterns
    - Captures exit codes
    - Timing information
    ↓
[CommandAnalyzer (Rust)]
    - Normalize command
    - Extract signature
    - Classify category
    - Summarize output
    - Extract tags
    ↓
[Terminal Intelligence Store]
    - Add to history
    - Persist to IndexedDB
    - Cache patterns
    - Update suggestions
    ↓
[UI Components]
    - Search Results
    - Pattern Recommendations
    - Performance Insights
    - Suggestion Panel
```

## Key Decisions

### 1. **Zustand for State Management**
**Decision**: Use Zustand over Redux/MobX  
**Rationale**:
- Lightweight, matches Troupe conventions
- Integrates with persist middleware
- Minimal boilerplate
- Strong TypeScript support

### 2. **Rust Backend Analysis**
**Decision**: Implement core analysis in Rust, not JavaScript  
**Rationale**:
- Handle large command histories efficiently
- CPU-intensive pattern detection
- Consistent analysis across desktop/server/web
- Future optimization opportunities

### 3. **Semantic Token Approach**
**Decision**: Extract semantic tokens instead of using embeddings  
**Rationale**:
- Works offline without ML models
- Deterministic and reproducible
- Lightweight: O(n) with small constant
- Clear to debug and extend

### 4. **Pattern Detection Window Size**
**Decision**: Search for patterns of 2-5 consecutive commands  
**Rationale**:
- 2: Too short, too many false positives
- 5: Practical workflow sequences (build→test→deploy)
- >5: Diminishing returns, O(n²) complexity
- Configurable via `commandPattern.windowSize`

### 5. **Local-First Storage**
**Decision**: IndexedDB by default, server-sync optional  
**Rationale**:
- Works offline
- User controls data
- No network latency for search
- Server-sync adds optional cloud features

### 6. **AI as Enhancement, Not Requirement**
**Decision**: Core features work without LLM backend  
**Rationale**:
- Always works for local analysis
- Suggestions optional and graceful fallback
- Respects privacy preferences
- Future-compatible with multiple AI providers

## Implementation Strategy

### Phase 1: Core System (This PR)
✅ Terminal Intelligence types and IPC contracts  
✅ Zustand store for state management  
✅ Rust CommandAnalyzer service  
✅ Command capture hook (useTerminalIntelligence)  
✅ UI: Command Search Panel  
✅ Documentation & ADR  

### Phase 2: AI Integration (Future)
- [ ] ACP agent integration for suggestions
- [ ] Output analysis using Claude/Gemini
- [ ] Pattern naming via LLM
- [ ] Intelligent workflow recommendations

### Phase 3: Analytics & Dashboard
- [ ] Performance metrics dashboard
- [ ] Cross-project analytics
- [ ] Workflow trend analysis
- [ ] Team-wide insights

### Phase 4: Automation
- [ ] Workflow recording & replay
- [ ] One-click workflow execution
- [ ] Command aliasing from patterns
- [ ] CI/CD integration

## Testing Strategy

### Unit Tests (Rust)
```rust
#[test]
fn test_normalize_command() { }

#[test]
fn test_classify_command() { }

#[test]
fn test_extract_signature() { }

#[test]
fn test_pattern_detection() { }
```

### Integration Tests
- Command capture and analysis flow
- Store persistence and retrieval
- Cross-terminal pattern detection
- UI component interactions

### E2E Tests
- Full workflow: execute → capture → analyze → suggest
- Multi-terminal coordination
- Real ACP agent integration

## Performance Implications

### Memory
- **Command History**: ~1KB per command × 10,000 max = ~10MB
- **Pattern Cache**: ~500B per pattern × 100 patterns = ~50KB
- **Store Overhead**: Zustand + persist middleware ~2MB

**Total**: ~12MB maximum reasonable footprint

### CPU
- **Analysis**: <10ms per command (Rust)
- **Search**: <100ms for 10,000 commands
- **Pattern Detection**: ~500ms (periodic)

### Network (Optional)
- Server-sync: Batch update ~100 commands/request
- No real-time sync needed (eventual consistency OK)

## Backwards Compatibility

- ✅ Zero breaking changes to existing Troupe API
- ✅ Optional feature, doesn't affect core functionality
- ✅ Graceful fallback if Rust backend unavailable
- ✅ Existing projects unaffected

## Security & Privacy

### Data Sensitivity
- Commands may contain sensitive info (passwords, tokens, etc.)
- Outputs may contain API keys or credentials

### Mitigations
- **Local-first**: No data leaves device by default
- **User Control**: Can exclude sensitive commands
- **Filtering**: Automatic detection of common secrets
- **Storage**: Encrypted on disk (leverages OS keychain)
- **Clearing**: Easy deletion of history per project

### Compliance
- GDPR compatible (user-controlled data)
- HIPAA compliant (no cloud by default)
- SOC2-ready architecture

## Failure Modes & Recovery

| Scenario | Impact | Recovery |
|----------|--------|----------|
| Rust backend crash | Search unavailable | Restart app, reload from IndexedDB |
| Storage corruption | Loss of history | Automatic cleanup, fresh start |
| Pattern detection timeout | Slight delay | Async, non-blocking operation |
| AI backend unavailable | No suggestions | Use local heuristics |

## Monitoring & Observability

### Logging
```rust
info!("Analyzed command: {} ({}ms)", command, duration);
debug!("Detected pattern: {} (confidence: {})", pattern.name, pattern.confidence);
warn!("Pattern detection timeout, skipping");
```

### Metrics to Track
- Commands analyzed per session
- Pattern detection success rate
- Search latency percentiles
- Storage usage trend

## Future Extensions

### 1. Cross-Project Insights
- Command usage across projects
- Team-wide patterns
- Best practices identification

### 2. Collaborative Features
- Share patterns with teammates
- Workflow recommendations from team
- Best practice library

### 3. Automation
- Record and replay workflows
- One-click execution of patterns
- Integration with CI/CD

### 4. Intelligence
- Anomaly detection (unusual commands)
- Security risk flagging
- Performance optimization AI

### 5. Mobile Enhancement
- Access pattern library on phone
- Quick command lookup
- One-tap command execution

## Alternative Approaches Considered

### 1. **Full ML Embeddings**
- ❌ Requires model shipping
- ❌ Large binary size
- ✅ Better semantic matching
- **Rejected**: Complexity vs. benefit

### 2. **Server-Only Analysis**
- ❌ Requires network
- ❌ Privacy concerns
- ❌ Latency for search
- **Rejected**: Local-first principle

### 3. **Command History in Git**
- ❌ Pollutes repository
- ❌ Breaks for non-git projects
- ✅ Version-controlled
- **Rejected**: Not project-specific

## References

- Troupe Terminal Architecture: `docs/architecture.md`
- ACP Agent Integration: `docs/agents.md`
- Component Conventions: `AGENTS.md`
- Terminal Intelligence Guide: `docs/terminal-intelligence.md`

## Conclusion

The Terminal Intelligence System brings sophisticated command analysis to Troupe while maintaining privacy, performance, and ease of use. By combining local semantic analysis with optional AI enhancement, it provides immediate value while enabling future sophisticated features.

The implementation prioritizes:
1. **Developer Experience**: Seamless, non-intrusive integration
2. **Privacy**: Local-first with user control
3. **Performance**: Efficient algorithms and async operations
4. **Extensibility**: Clear APIs for future AI integration
5. **Reliability**: Graceful degradation and error handling

This creates a foundation for Troupe to become an increasingly intelligent development environment that learns from and optimizes developer workflows.

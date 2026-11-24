# The ACP chat model in Rust

Plan section R6, first Build box: port the `acp-store` slices into
`apps/rune/crates/model/src/acp/` and replay vectors recorded from the
TypeScript store before any view is written. This file is the map: which
TypeScript module becomes which Rust module, what is state, what is an effect
the app crate runs, what stays React glue, which unit suites give vectors, and
the order of the PR stack.

## The shape

The TypeScript store is one Zustand store (`useAcpStore`) built from slices
that read and write each other's fields: a `user_prompt` echo writes
`messages` and `sessions`; a `prompt_complete` writes `sessions`, `messages`,
`plans`, `promptQueues` and the pending prompts. Two shapes were weighed.

- **One state, one step (chosen).** `acp::State` mirrors the store's data
  fields. `acp::step(state, Event, env) -> Vec<Effect>` applies one event. Each
  TypeScript slice becomes one Rust module of `impl State` handlers, so a
  handler that crosses slices writes the fields it needs, the way the
  TypeScript one does. The vector snapshot is one JSON object, and the replay
  compares it after every step.
- **One state per slice, composed.** Each slice owns its struct and `step`; a
  router passes cross-slice reads in. Rejected: most handlers cross two or
  three slices, so every call would carry a bundle of borrowed views of the
  others, and the snapshot would have to be stitched back together for the
  replay. It hides nothing the first shape exposes.

Module-level TypeScript singletons that change what a handler does (the seq
counter, the history watermarks, the live-turn replay floors, the clamp-log
dedup set) are fields of `State::book`, not globals. They serialize under
`book` so a vector starts from the recorded values.

The model never reads a clock or draws randomness. `Env` carries `now` (epoch
ms), an [`Ids`](../../apps/rune/crates/model/src/lib.rs) source and the history
mode (`server` or not). The app passes the wall clock and random ids; the
replay passes the recorded ones.

Open agent payloads stay open. `ContentBlock` and `ToolCall` carry an index
signature in TypeScript (`[k: string]: unknown`) and arrive verbatim from the
agent, so the Rust types keep them as JSON objects with typed accessors for
the fields the model reads. Session, message and usage records are typed
structs, with a flattened `rest` map for the fields a later slice owns.

String bounds count UTF-16 code units, as JavaScript's `length` and `slice`
do. Serialized-size bounds measure the JSON as `JSON.stringify` writes it,
numbers included (`1`, not `1.0`). A cut that splits a surrogate pair drops
the whole pair, since a Rust string cannot hold the lone half JavaScript keeps.

## State, effect, glue

| Kind | What it is | Where it goes |
|---|---|---|
| State | Plain data a handler reads or writes: sessions, messages, tool calls, plans, commands, usage, pending prompts, queues, the agent roster, MCP list and probe status, the bookkeeping maps | `acp::State`, ported and replayed |
| Effect | A call outside the store: a socket command (`send_prompt`, `cancel`, `set_mode`, `respond_permission`), a persistence write (session index, payload, composer record), a log line, a timer (deferred turn end, reconcile resend, flush clock), a toast, a route or workspace-tab change | an `acp::Effect` the app crate runs |
| Glue | React and browser plumbing: the rAF flush clock, `visibilitychange`, the coalescing buffer and its text-delta sealing, `useAcpStore` selectors, `initAcpEventListeners` wiring | not ported; Rune applies each event at once and redraws on its own clock |

The coalescing buffer is glue because the vectors flush after every step: the
state after a flush equals the state after applying the same events at once.
Its window trim (`trimLiveWindow`) is not glue. It needs the payload cache, so
it is an effect pair (probe, then trim) in the transcript-window PR.

## Module map

TypeScript lines are whole files, comments included. Every path below is under
`src/renderer/stores/acp-store/` unless it starts with `lib/` or `components/`
(under `src/renderer/`).

| TypeScript | Lines | Rust module | State / effect / glue | PR |
|---|---:|---|---|---|
| `types.ts` | 1,279 | `acp/types.rs` (records), `acp/mod.rs` (`State`, `Event`, `Effect`) | state types; the action signatures become `Event` variants | 1, grows per PR |
| `slices/transcript.ts` event handlers (`_onUserPrompt`, `_onMessageChunk`, `_onToolCall`, `_onToolCallUpdate`, `_onPlanUpdate`, `_onCommandsUpdate`, `_onSessionInfoUpdate`, `_onUsageUpdate`) and the live clamps | 1,788 | `acp/transcript.rs`, `acp/tool_call.rs`, `acp/clamp.rs` | state; logs and `persistSession` are effects | 1 |
| `slices/transcript.ts` window (`loadOlderMessages`, `trimLiveWindow`, backfill counts) | (same file) | `acp/window.rs` | state plus a payload-read effect | 5 |
| `slices/transcript.ts` `_onModeUpdate`, `_onConfigOptionsUpdate` | (same file) | `acp/controls.rs` | state; composer-record write is an effect | 4 |
| `slices/transcript-flush-clock.ts` | 149 | none | glue | none |
| `lib/plan-session.ts` (plan tool, merge, classify) | 388 | `acp/plan.rs` | state | 1 |
| `components/chat/terminal-output.ts` | 48 | `acp/tool_call.rs` | state | 1 |
| `lib/github-handoff/read-only-guard.ts` `splitReadOnlyGuard` | 61 | `acp/transcript.rs` | state | 1 |
| `components/chat/handoff-summary.ts` `stripHandoffPreamble` | 491 | `acp/transcript.rs` | state | 1 |
| `lib/skills-wire-reverse.ts` `wireBlocksToDisplay` | 227 | `composer` (R6 composer unit) | state | composer unit |
| `helpers.ts` | 2,523 | split by caller: transcript predicates to `transcript.rs`, turn-end helpers to `turn.rs`, prepared-slot and launch helpers to `launch.rs` | state | 1 to 7 |
| `shared-state.ts` (seq counter, watermarks, replay floors) | 1,149 | `acp/book.rs` (`State::book`) | state | 1 |
| `shared-state.ts` (`persistSession`, collectors, warm pool, spawn signatures, `ensureLiveAgent`) | (same file) | effects; the warm-pool and spawn maps go to `launch.rs` | effect | 6, 7 |
| `subagent-transcript.ts`, `subagent-lifecycle.ts` (record parse, ingest, child text) | 524 | `acp/subagent.rs` | state | 2 |
| `subagent-lifecycle.ts` `cancelRunningSubagents` | (same) | `acp/subagent.rs` | state | 3 |
| `child-approvals.ts` | 66 | `acp/approvals.rs` | state | 2 |
| `live-turn.ts` | 153 | `acp/turn.rs` | state | 3 |
| `slices/prompt.ts` permission, question and elicitation handlers | 1,671 | `acp/approvals.rs` | state; `respond_*` are socket effects | 2 |
| `slices/prompt.ts` (`_onPromptComplete`, `_onAgentWorking`, `_onBackgroundCommands`) | (same) | `acp/turn.rs` | state; the queue flush is an effect | 3 |
| `slices/prompt.ts` (send, cancel, queue, send-now) and `../prompt-queue-orchestration.ts` | (same) + 277 | `acp/prompt.rs`, `acp/queue.rs` | state; `send_prompt`/`cancel` are ledgered socket effects | 4 |
| `slices/prompt-reconcile.ts` | 173 | `acp/prompt.rs` | state plus a resend timer effect | 4 |
| `pending-permission-sync.ts` | 146 | `acp/approvals.rs` | state | 3 |
| `host-session-reports.ts` | 62 | `acp/turn.rs` | state | 3 |
| `rate-limit.ts` | 34 | `acp/usage.rs` | state | 1 |
| `session-controls.ts`, `composer-options.ts`, `model-catalog.ts` | 534 | `acp/controls.rs` | state; the composer record is a persistence effect | 4 |
| `slices/session.ts` (`_onSessionCreated`, `_onSessionClosed`, `createSession`, `closeSession`, index load and snapshot, history open, retry) | 2,879 | `acp/session.rs`, `acp/history.rs` | state; payload reads, index writes, `session/load` are effects | 5 |
| `slices/agent.ts` (roster events, spawn, kill, sign-in, browser consent) and `agent-roster-watch.ts` | 1,554 | `acp/agents.rs` | state; spawn and kill are socket effects | 6 |
| `browser-agent-tabs.ts` | 77 | `acp/agents.rs` | state; the workspace tab is an effect | 6 |
| `slices/agent.ts` `_onBrowserConsentTicket`, `_onBrowserTicketClosed` and `lib/browser-consent-api.ts` `parseBrowserConsentTicket` | (part) | `acp/approvals.rs` | state; revealing the chat tab is an effect | 2 |
| `slices/launch.ts`, `chat-reuse-scope.ts`, `ephemeral-ids.ts`, `registry-mcp-fingerprints.ts` | 1,139 | `acp/launch.rs` | state; prepare and claim are socket effects | 7 |
| `slices/config.ts` | 344 | `acp/launch.rs` | state; config writes are persistence effects | 7 |
| `slices/switch.ts`, `slices/switch-seam.ts` | 943 | `acp/switch.rs` | state; `record_agent_switch` and payload reads are effects | 8 |
| `slices/mcp.ts` | 682 | `acp/mcp.rs` | state; probe, OAuth and writes are effects | 9 |
| `slices/import-sessions.ts` | 168 | `acp/history.rs` | effect-driven (a host loop); the candidate filter is state | 9 |
| `slices/misc.ts` (commit message, terminal assist, bug report collectors) | 537 | `acp/oneshot.rs` | one-shot sessions whose chunks feed a collector, never a transcript | 9 |
| `reload-guard.ts`, `chat-tab-prune.ts` | 63 | `acp/session.rs` | state; the tab prune is a workspace effect | 5 |
| `index.ts` (store assembly, `initAcpEventListeners`, `_reset*ForTesting`) | 823 | none | glue | none |

## Vectors

The recorder (`tools/parity/vectors/record-acp.test.ts`) runs the real
`useAcpStore` with the same module mocks the unit suites use, on scripted
scenarios. Each scenario is a case of a unit suite, named after it. It
flushes the coalescing buffer after every step and snapshots the data fields
plus `book`. The Rust replay is `apps/rune/crates/model/tests/acp_vectors.rs`.

The brief asked to capture the unit suites' own runs. Those suites seed state
with `setState` between calls, read state the vector does not snapshot, and
need per-file mocks that vitest hoists per file, so a capture hook would need
an edit to every suite. Scenarios restate the cases instead, and every scenario
names its source test.

| Suite | Cases it gives | PR |
|---|---|---|
| `events.test.ts` | chunk merge, message ids, tool boundary, roles, closed sessions, user echo, tool upsert, update merge, plan, commands, usage, session info | 1 |
| `replay-dedup.test.ts` | history watermark drops, replay replace, server-history window | 1 |
| `tool-call-stamps.test.ts` | seq and timestamp stamps, legacy adoption | 1 |
| `core.test.ts` | clamps of oversized tool fields and content blocks | 1 |
| `subagent-transcript.test.ts`, `subagent-lifecycle.test.ts` | record parse, child chunks, dispatch cards | 2 |
| `child-approvals.test.ts`, `browser-consent.test.ts`, `events.test.ts` (requests) | approval ownership, consent tickets | 2 |
| `pending-permission-sync.test.ts`, `live-turn.test.ts`, `live-turn-attach-replay.test.ts`, `derive-open-turn.test.ts`, `events.test.ts` (turn end) | resubscribe install, turn end, reattach | 3 |
| `prompting.test.ts`, `prompt-reconcile.test.ts`, `composer.test.ts` | send, queue, flush, reconcile | 4 |
| `session-controls.test.ts`, `options-cache-hydration.test.ts`, `model-catalog.test.ts` | controls, echo guard | 4 |
| `transcript-window.test.ts`, `closed-transcript-eviction.test.ts` | live window, backfill, eviction | 5 |
| `lifecycle.test.ts`, `history-session.test.ts`, `persistence.test.ts`, `stale-cache-reopen.test.ts`, `dead-turn-reopen.test.ts`, `turn-error-reopen.test.ts`, `discovered-reopen-turn-active.test.ts`, `continued-from.test.ts`, `created-at-invariant.test.ts`, `chat-title-turn-end.test.ts` | session lifecycle and reopen | 5 |
| `agent-roster-watch.test.ts`, `auth-recovery.test.ts`, `agent-browser-pane.test.ts`, `browser-consent.test.ts`, `agent-update.test.ts` | roster, auth, browser | 6 |
| `spawn-prepare.test.ts`, `warm-pool.test.ts`, `host-agent-reuse.test.ts`, `multi-project.test.ts`, `resolve-spawn-project.test.ts`, `ensure-live-agent-*.test.ts`, `quick-chat-persist.test.ts`, `agent-config-attribution.test.ts`, `shared-config-sync.test.ts` | launch and reuse | 7 |
| `agent-switch.test.ts`, `selection-context-twin.test.ts` | switch | 8 |
| `mcp-*.test.ts`, `import-sessions.test.ts`, `discovery.test.ts`, `plans-commands.test.ts`, `assist-terminal.test.ts`, `reconnect-priority.test.ts`, `chat-tab-prune.test.ts`, `transcript-flush-*.test.ts` | MCP, import, discovery, one-shots; the flush tests are glue and give none | 9 |

## PR stack

Each PR stays under 50 files, adds its vectors, and is green on its own.

1. Transcript core. This map, the recorder and replay harness for the acp
   store, `acp::State`, the event reducer for prompts, chunks, tool calls,
   tool updates, plans (including plan tools), commands, usage and session
   info, the live clamps, the history watermark. Also the `mark_chat_read`
   reply split in `troupe-protocol` (R1b review follow-up).
2. Subagents and approvals. The host's subagent record and control marker,
   the parent link and the wire subagent messages on every tool event, a
   subagent's text kept on its dispatch call, permission, question and
   elicitation requests, browser consent tickets, and the dismissal of a
   settled orchestrated run's prompts.
3. Turn end. `_onPromptComplete`, agent working, background commands and the
   host session reports, the resubscribe install of pending prompts, the
   cancel of running subagents, live-turn reattach.
4. Prompt and queue, session controls. Send, cancel, queue mutations,
   reconcile, mode, model and config options with the echo guard.
5. Session lifecycle and history. Created, closed, index load, history open,
   live window, eviction.
6. Agent roster, auth and browser.
7. Launch, prepare and warm pool, agent configs.
8. In-chat agent switch.
9. MCP, import, discovery, one-shot collectors.

## Not ported yet, on purpose

- `wireBlocksToDisplay` on the `user_prompt` echo. It reverses the composer's
  wire framing (skill paths, file tokens, conversation envelopes) and belongs
  to the composer unit, which ports `lib/composer/`. The model keeps the
  echo's blocks as sent; the vectors use prompts without framing.
- The prompt-reconcile bookkeeping on the echo (`activeWirePromptTurns`,
  `clearPromptReconcileForTurn`, `handoffOnlyTurnIds`). PR 4.
- The one-shot collectors that swallow chunks, tool calls and requests of
  commit-message, terminal assist and bug-report sessions. PR 9.
- The live-window trim and the tool-call trim at flush. PR 5.
- Switch markers in `maxPayloadSeq`. PR 8.

## Recorder limits

- The TypeScript clamp log remembers ids across vectors, and only a stored
  call's id can be forgotten from outside. Vectors that log a clamp use ids
  no other vector uses.
- The TypeScript seq counter only moves up, so each vector starts on the next
  hundred and records the start in `book.seq`. Adding a scenario shifts the
  seqs of the ones after it; re-record and commit the whole file.

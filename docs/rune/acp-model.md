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

Plan entries and the other open payloads keep their key order
(`serde_json`'s `preserve_order`), as JavaScript objects do: the plan
snapshot fence is `JSON.stringify` of the entries as the agent sent them.

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
| `slices/transcript.ts` window (`loadOlderMessages`, `trimLiveWindow`, backfill counts); `helpers.ts` `trimLiveToolCalls` | (same file) | `acp/window.rs`, `acp/diff_stat.rs` | state plus a payload-read effect | 6 |
| `slices/transcript.ts` `_onModeUpdate`, `_onConfigOptionsUpdate`; `helpers.ts` `mergeAgentConfigOptions`, `cacheOptionsFromSession`, `writeAgentOptionsCache`; `model-catalog.ts` `modelCatalogFromOptions`; the model helpers of `components/chat/chat-input-bar-config.ts` | (parts) | `acp/controls.rs` | state; the model list save and the saved-pick heal are effects | 4 |
| `slices/transcript-flush-clock.ts` | 149 | none | glue | none |
| `lib/plan-session.ts` (plan tool, merge, classify) | 388 | `acp/plan.rs` | state | 1 |
| `components/chat/terminal-output.ts` | 48 | `acp/tool_call.rs` | state | 1 |
| `lib/github-handoff/read-only-guard.ts` `splitReadOnlyGuard` | 61 | `acp/transcript.rs` | state | 1 |
| `components/chat/handoff-summary.ts` `stripHandoffPreamble` | 491 | `acp/transcript.rs` | state | 1 |
| `lib/skills-wire-reverse.ts` `wireBlocksToDisplay` | 227 | `composer` (R6 composer unit) | state | composer unit |
| `helpers.ts` | 2,523 | split by caller: transcript predicates to `transcript.rs`, turn-end helpers to `turn.rs`, prepared-slot and launch helpers to `launch.rs` | state | 1 to 8 |
| `shared-state.ts` (seq counter, watermarks, replay floors) | 1,149 | `acp/book.rs` (`State::book`) | state | 1 |
| `shared-state.ts` (`persistSession`, collectors, warm pool, spawn signatures, `ensureLiveAgent`) | (same file) | effects; the warm-pool and spawn maps go to `launch.rs` | effect | 7, 8 |
| `subagent-transcript.ts`, `subagent-lifecycle.ts` (record parse, ingest, child text) | 524 | `acp/subagent.rs` | state | 2 |
| `subagent-lifecycle.ts` `cancelRunningSubagents` | (same) | `acp/subagent.rs` | state | 6 |
| `child-approvals.ts` | 66 | `acp/approvals.rs` | state | 2 |
| `live-turn.ts` | 153 | `acp/session.rs` | state | 6 |
| `slices/prompt.ts` permission, question and elicitation handlers | 1,671 | `acp/approvals.rs` | state; `respond_*` are socket effects | 2 |
| `slices/prompt.ts` (`_onPromptComplete`, `scheduleTurnEnd`, `_onAgentWorking`, `_onBackgroundCommands`) and `lib/agent-chat-notify.ts` `bumpTurnEndNotice` | (same) | `acp/turn.rs` | state; the deferred close and the queued send are effects | 3 |
| `slices/prompt.ts` (send, cancel, queue, send-now, busy retry) and `../prompt-queue-orchestration.ts` | (same) + 277 | `acp/prompt.rs` | state; the dispatch, the cancel and the retry timer are effects | 5 |
| `slices/prompt-reconcile.ts` | 173 | the client crate's command ledger, plus a `waiting to confirm` state | the resend is the ledger's | 11 |
| `pending-permission-sync.ts` | 146 | `acp/approvals.rs` | state | 6 |
| `host-session-reports.ts` | 62 | `acp/turn.rs` | state | 3 |
| `rate-limit.ts` | 34 | `acp/usage.rs` | state | 1 |
| `session-controls.ts`, `composer-options.ts` (`setMode`, `setModel`, `setConfigOption`) | 404 | `acp/picks.rs` | state; the composer record and the set commands are effects | 11a |
| `slices/prompt.ts` `pickModel`; `helpers.ts` `reapplyPersistedComposerOptions`, `applyOptionValues`, `waitForReadvertisedOptions`; `composer-options.ts` `savedModelValues`, `savedAgentWideValues`, `warnOnce` | (parts) | `acp/restore.rs` | state; the record read, the set commands and the re-advertise wait are effects | 11b |
| `slices/session.ts` (`_onSessionCreated`, `_onSessionClosed`, `createSession`, `closeSession`, index load and snapshot, history open, retry) | 2,879 | `acp/session.rs`, `acp/history.rs` | state; payload reads, index writes, `session/load` are effects | 6 |
| `slices/agent.ts` (roster events, browser consent) and `agent-roster-watch.ts` | 1,554 | `acp/agents.rs` | state | 7a |
| `slices/agent.ts` (spawn, kill, sign-in, sign-out, the sign-in prompt answer, the auth bookkeeping) | (same file) | `acp/agent_commands.rs` | state; each command is an effect with an outcome event | 7b |
| `agent-roster-watch.ts` | 127 | `acp/roster.rs` | state; the policy and automation hints and the toasts are effects | 7 |
| `browser-agent-tabs.ts` | 77 | `acp/roster.rs` | effects: the Stop owner, and on the desktop the workspace tab | 7 |
| `slices/agent.ts` `_onBrowserConsentTicket`, `_onBrowserTicketClosed` and `lib/browser-consent-api.ts` `parseBrowserConsentTicket` | (part) | `acp/approvals.rs` | state; revealing the chat tab is an effect | 2 |
| `slices/launch.ts`, `chat-reuse-scope.ts`, `ephemeral-ids.ts`, `registry-mcp-fingerprints.ts` | 1,139 | `acp/launch.rs` | state; prepare and claim are socket effects | 8 |
| `slices/config.ts` | 344 | `acp/launch.rs` | state; config writes are persistence effects | 8 |
| `slices/switch.ts`, `slices/switch-seam.ts` | 943 | `acp/switch.rs` | state; `record_agent_switch` and payload reads are effects | 9 |
| `slices/mcp.ts`; `lib/acp-mcp-persistence.ts` `catalogInstallEntry`; `lib/mcp-install-request.ts` | 682 | `acp/mcp.rs` (registry, probes), `acp/mcp_oauth.rs`, `acp/mcp_install.rs` | state; probe, OAuth and writes are effects | 10 |
| `slices/import-sessions.ts` | 168 | `acp/history.rs` | effect-driven (a host loop); the candidate filter is state | 10 |
| `slices/misc.ts` (commit message, terminal assist, bug report collectors) | 537 | `acp/oneshot.rs` | one-shot sessions whose chunks feed a collector, never a transcript | 10 |
| `reload-guard.ts`, `chat-tab-prune.ts` | 63 | `acp/session.rs` | state; the tab prune is a workspace effect | 6 |
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
| `events.test.ts` (turn end), `prompting.test.ts` (turn end), `plans-commands.test.ts` (plan snapshot) | turn end, stop notes, plan settle and snapshot, agent working, background commands | 3 |
| `pending-permission-sync.test.ts`, `live-turn.test.ts`, `live-turn-attach-replay.test.ts`, `derive-open-turn.test.ts` | resubscribe install, reattach | 6 |
| `prompting.test.ts`, `prompt-reconcile.test.ts`, `composer.test.ts` | send, queue, flush, reconcile | 5 |
| `session-controls.test.ts`, `options-cache-hydration.test.ts`, `model-catalog.test.ts` | controls, echo guard | 4 |
| `transcript-window.test.ts`, `closed-transcript-eviction.test.ts` | live window, backfill, eviction | 6 |
| `lifecycle.test.ts`, `history-session.test.ts`, `persistence.test.ts`, `stale-cache-reopen.test.ts`, `dead-turn-reopen.test.ts`, `turn-error-reopen.test.ts`, `discovered-reopen-turn-active.test.ts`, `continued-from.test.ts`, `created-at-invariant.test.ts`, `chat-title-turn-end.test.ts` | session lifecycle and reopen | 6 |
| `agent-roster-watch.test.ts`, `auth-recovery.test.ts`, `agent-browser-pane.test.ts`, `browser-consent.test.ts`, `agent-update.test.ts` | roster, auth, browser | 7 |
| `spawn-prepare.test.ts`, `warm-pool.test.ts`, `host-agent-reuse.test.ts`, `multi-project.test.ts`, `resolve-spawn-project.test.ts`, `ensure-live-agent-*.test.ts`, `quick-chat-persist.test.ts`, `agent-config-attribution.test.ts`, `shared-config-sync.test.ts` | launch and reuse | 8 |
| `agent-switch.test.ts`, `selection-context-twin.test.ts` | switch | 9 |
| `mcp-*.test.ts`, `import-sessions.test.ts`, `discovery.test.ts`, `plans-commands.test.ts`, `assist-terminal.test.ts`, `reconnect-priority.test.ts`, `chat-tab-prune.test.ts`, `transcript-flush-*.test.ts` | MCP, import, discovery, one-shots; the flush tests are glue and give none | 10 |

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
3. Turn end. `_onPromptComplete` and the deferred close it schedules (an
   `endTurn` effect the app feeds back as the `turnEnd` event), stop-reason
   notes, the plan settle and snapshot fence, agent working, background
   commands and the host session reports. The close flushes the next queued
   prompt, which PR 5 sends.
4. Session controls from the agent. `mode_update` and
   `config_option_update` with the creation-default echo guard and the
   sticky model value, the shown model copied onto `models` and the index
   row, and the per-config options cache with two effects: save the model
   list, and heal the user's saved picks.
5. Sending. `sendPrompt` and `sendPromptBlocks` (queue behind a busy chat
   or open the turn with the user's bubble), the dispatch outcomes (an
   answer, a busy host with the busy-retry ladder, a definite failure), the
   queue actions, send-now (stopping a running turn first), and Stop. See
   "Sending as two steps" below.
6. Session lifecycle and history. Created, closed, index load, history open,
   live window, eviction, the resubscribe install of pending prompts, the
   cancel of running subagents, live-turn reattach. Split in two:
   - 6a (R6a-2). `session_created` (the stub, the advertised options added
     to a known session, the creation baseline, the options cache),
     `session_closed` (the old-process guard, the cancel of running
     subagents, prompts and warm slots dropped, the transcript kept for an
     open tab or freed), `setActiveSession` and `markResumeFailed`. The
     desktop's index reload is the `refreshSessionIndex` effect; the tabs a
     chat shows are `Book::open_chat_tabs`, which the app keeps equal to the
     workspace. A close (and a disconnect that empties a chat) persists
     first and frees the transcript only after: `persistSession` comes
     before `dropSessionTranscript`, which the app sends back as an event
     once the persist read the state.
   - 6b, first part (R6a-2). `applySessionIndexSnapshot`, the answer to
     `refreshSessionIndex`: the host's rows merged with the local
     projection (`mergeSessionIndexEntries`), live turns stamped, the
     placeholder titles of live chats replaced by the index's, handoff
     titles cleaned, and the tabs of chats neither live nor indexed closed
     (`closeChatTab`). Not here: the untitled-counter rebase (it only
     names chats `createSession` makes, PR 8) and the dropped-launch
     recovery (PR 8). Events apply in order, so the load generation guard
     has no counterpart.
   - 6b, live window (R6a-4). `acp/window.rs`: the trims a flush runs on
     the session a chunk, tool call or tool update coalesced into (not a
     replaying session, not a covered event). Messages over 300 keep the
     newest and the streaming tail, but only once the session's history is
     known durable: the first time over the bound the model sends
     `readSessionPayload` (the durability probe), and its answer
     (`sessionPayloadRead`) marks the session durable, untrimmable (no
     history, logged) or neither (a failed read, logged, probed again on the
     next flush). Tool calls over 500 drop the oldest finished ones, never a
     subagent dispatch; a dropped edit, delete or move leaves a path-only
     summary with its line counts (`acp/diff_stat.rs`, a copy of the host's
     `diff_stat.rs`). `clearSessionBackfill` forgets the backfill and trims
     at once. The payload cache and its pins are the app's: the model only
     remembers which sessions are durable, so a cache eviction does not make
     it probe again where the TypeScript would. A delete forgets it, as the
     TypeScript delete evicts the cached payload.
   - 6b, close and delete (R6a-4, `acp/close.rs`). `closeSession` ends an
     open chat's agent session (`closeAgentSession`), then closes the chat,
     drops its warm slot, its prompts, its queue and its queue hold,
     persists it and frees its transcript. `deleteHistorySession` asks an
     agent that advertises `session/delete` (or one the model knows nothing
     about, for a chat only the index holds) to delete it
     (`deleteAgentSession`), deletes the durable history
     (`deleteSessionPayload`) and only then drops the chat, its index row,
     its warm slot, its transcript and its tab. Both, and `_onSessionClosed`,
     delete the app's staged files for the chat (`deleteSessionTempFiles`).
     Not here: the warm-pool session that closes empty (`ephemeralSessionIds`,
     PR 8), the reopen and restore invalidation (history open), and the
     launch cancellation a delete during a launch leaves (`finalizeChatLaunch`,
     PR 8).
   - 6c, `createSession` (R6a-6, `acp/create.rs`). The call is the
     `newSession` effect (the `kind` option is `sessionKind`, since `kind`
     names the effect), answered by `agentCommandDone` with the host's
     reply or `agentCommandFailed`; the action answers `actionResolved`
     with the session id. Without explicit servers it starts the enabled
     registry servers the agent can run (`selectMcpServersForAgent`; all of
     them before the agent reports capabilities) and toasts the skipped
     ones. It sends the Project's folder and worktrees as extra roots to an
     agent that advertises `additionalDirectories`. A `session/new` refused
     for a sign-in goes through `withAuthRetry` on the picker surface: an
     agent with several methods and none remembered rejects with
     `AmbiguousAuthError`, which the launcher's picker reads. The reply
     rebuilds the chat record, keeping what events set while the call ran
     (title, a closed status, the open turn, the creation baseline,
     `createdAt`, the host's `agent_working`); a non-ephemeral chat is
     persisted and becomes the active one if none is; the options cache is
     filled either way. A failure the setup classifier files under
     `transport` forgets the agent's sign-in and kills it (`killAgent`
     with `Killer::Evict`) before the caller sees the error; one under
     `auth` forgets the sign-in. A resend the host answered from its
     records (`lookup`) fails for an ephemeral session; any other is opened
     from history (6d) before the create answers. Not here: the registry server
     fingerprints (`recordRegistryMcpServers`, read by the warm-pool
     claim), `ephemeralSessionIds` (PR 8), the reopen generation
     (`invalidateSessionReopen`, history open) and `pendingRestartVersions`
     (the agent update slice).
   - 6d, history open (R6a-6, `acp/history.rs`, `acp/history_install.rs`).
     `openHistorySession` on the desktop path. A live chat (not closed, not
     in error) answers at once; a second open of the same chat joins the one
     in flight. The open reads the full payload (`readSessionPayload`) and
     installs the chat closed: the controls the client lacks come from the
     payload, hidden turns and their tool cards are dropped, user bubbles
     lose the read-only guard and a handoff preamble, the open turn comes
     from `turnActive` or a trailing unanswered prompt, the last turn's
     error (provider wording, stop-reason note) shows, a placeholder title
     gives way to the index's, the payload's commands apply, and the newest
     `termul-plan` fence becomes the plan (settled unless the turn runs; a
     malformed one is logged). A turn the persisted agent still runs is
     attached (`acp.attachLiveTurn`, with the replay floor below its
     prompt); a turn no agent runs is cleared. Otherwise `ensureLiveAgent`
     (`Caller::HistoryOpen`, silent, chat holder) gives the agent, and
     `decideResume` picks `resumeSession`, `loadSession` or nothing. Both
     go through `withAuthRetry` on the text surface. The reply's controls
     apply where no event changed them meanwhile; a load that replayed
     nothing closes its window at once. A failure restores the payload's
     transcript and shows `Resume failed: …`; the host's
     `ACP_REOPEN_TURN_ACTIVE` attaches instead; a resume that misses the
     session falls back to one load. The replay window closes on
     `endReplay`, which the app sends back as `replayEnd` after the queued
     events, and a titled chat in the index is persisted then.
     `Book::history` keeps the reopen generation per session
     (`invalidateSessionReopen`, also bumped by `createSession`), and a
     prompt queued meanwhile goes out when the open ends. Not here: the
     tail-first fetch (`loadSessionPayloadTail`; the full payload is read),
     the web client's adoption of a host-owned agent and
     `ACP_SESSION_OWNED_BY_OTHER`, the switch redirect and re-splice, the
     config reload, the Claude, Codex and Antigravity account reuse scope,
     the reopened chat's own model restore (`restoreReopenedChatSelection`,
     it needs `applyPendingLauncherOptions`), the 3 s capability wait (an
     agent without capabilities reopens locally), the durable plan replay
     from plan tool calls, the switch chain edges and the restore preload
     marks. A `createSession` the host answered from its records
     (`lookup`) opens the session it names this way and answers once the
     open did, or fails with the open's error.
   - 6e, `loadOlderMessages` (R6a-6, `acp/older.rs`). The page before the
     live window's head comes from the payload read (`readSessionPayload`);
     a cached copy that stops short of the head is read again from the host
     first (`reloadSessionPayload`, answered by `context.hostPayloads`). A
     head missing from the payload anchors by its host seq. The page drops
     hidden turns and the rows the window already shows, and a head
     anchored on its renderer seq folds its persisted twins by host seq or
     by text. The text compare is on the text as sent: the composer's
     wire-to-display reversal is not ported. `backfillCounts` keeps the
     page in the window. Not here: paging into the bands a live switch
     spliced before the chat's head, which the model never makes.
   - 6b, rest. The resubscribe install, live-turn reattach.
7. Agent roster, auth and browser. Split in two:
   - 7a (R6a-2). `agent_spawned` (the spawn response's fields win, an early
     browser-open is promoted), `agent_error` (the turn ends, the provider
     failure text, the rejected rate limit, an auth-required agent marked;
     since #2032 an Antigravity plan limit counts only for a chat whose
     config, by reuse key or index row, is Antigravity's, and carries its
     reset time),
     `agent_crashed` (the chats die, launch markers and warm slots go,
     running subagents end), `agent_disconnected` (a kill, reap or close
     drops the agent and its reuse keys; a crash keeps `error`), the
     browser-open and sign-in prompts and `clearPendingSignIn`. Not yet:
     the one-shot collectors (PR 10) and the warm-pool drop of an empty
     session (PR 8).
   - 7b, first part (R6a-3). `spawnAgent`, `killAgent`,
     `authenticateAgent`, `logoutAgent`, `respondAgentSignIn` and
     `completeBrowserAuth` (`acp/agent_commands.rs`). Each command is an
     effect (`spawnAgent`, `killAgent`, `authenticate`, `logout`,
     `respondElicitation`) the app answers with `agentCommandDone` and the
     reply, or `agentCommandFailed` and the error's message. The auth
     bookkeeping is in `Book`: `authenticatedAgents` and
     `rememberedAuthMethods` (serialized, so the vectors check them), the
     authenticate in flight per agent and method (a click on the same
     method shares it and fails with it), the last method tried, and the
     flight whose first browser open may open by itself (`persistAuthMethods`
     saves the remembered ones). The auth flight closes the 7a gap: the
     first browser open of a sign-in now sets `autoOpenBrowserUrl`. A
     disconnect forgets the agent's sign-in. Not here: the re-prepare of
     chats that failed on auth after `completeBrowserAuth` (it needs
     `prepareChat`, PR 8), `stoppingAgentIds` (read only by the warm-pool
     reap, PR 8), and the sign-in before a session call
     (`authenticateBeforeSession`, `withAuthRetry`, PR 8).
   - 7c (R6a-5). The sign-in on demand (`acp/auth_retry.rs`):
     `withAuthRetry` and `authenticateBeforeSession` around `setMode` and
     `setModel`, the two session calls the model sends so far. A set
     command that fails with the auth-required prefix, a signed-out
     Account, or wording `classifySetupError` files under `auth`
     (`is_auth_retriable`, checked against the TypeScript classifier's
     answers in `auth_retry_tests.rs`) signs in and is sent once more
     (`retried`, not serialized). The method: none when the agent is
     already signed in or the host prepared its auth, the only usable one,
     or the remembered one (a stale or non-agent memory is forgotten and
     the user is sent to a new chat's picker). Factory Droid reads whether
     the host stores its key (`readFactoryKeyStatus`, answered by
     `factoryKeyStatus`) and otherwise leaves a several-method sign-in to
     the banner. The authenticate shares the click's in-flight one of the
     same agent and method, and a click joining it fails with its error.
     A retry that fails again forgets the remembered method. Not here: the
     other session calls (`session/new`, `load`, `resume`; discovery wraps `list`), which
     their slices wrap when they are ported, and the forget tombstone the
     shared config sync reads.
   - 7b, rest (R6a-4). The host's `agents` watch and the agent browser
     tabs (`acp/roster.rs`). The watch's full state (`agentsWatchState`)
     spawns the agents the model lacks, disconnects the connected ones the
     host dropped, and asks the policy and automation views to re-read. A
     delta (`agentsWatchDelta`) goes to the handler the desktop event
     reaches; a crash or error also toasts. The browser policy and
     automation hints pass through as effects. `_onBrowserAgentTab` and
     `_onBrowserTabClosed` track the tab's Stop owner and, when
     `Book::desktop`, open or close the workspace tab; an open the history
     covers is skipped. A delta whose payload its handler cannot read is
     logged and dropped, where the TypeScript hands it on unchecked.
8. Launch, prepare and warm pool, agent configs.
   - 8a (R6a-4). The launch placeholder (`acp/launch.rs`):
     `createLaunchPlaceholder` (an initializing chat, with the launcher's
     first prompt shown and its turn open), `discardLaunchPlaceholder` (the
     chat, its transcript, its queue and the active mark go),
     `seedLaunchUserMessage` (the first prompt opens the turn once; a chat
     already showing a prompt only reopens it) and `clearLaunchingSession`.
   - 8b (R6a-5). `ensureLiveAgent` (`acp/ensure.rs`) and its first caller,
     `prewarmAgent`. The launch comes from the saved config (a legacy id
     such as `opencode-v2` launches the config it resolves to,
     `acp/spawn_env.rs`), the Project that holds the cwd or the one named,
     and the env the Project adds under the agent's own; its reuse key is
     `acp/reuse_key.rs`. A connected agent on the key is reused; one whose
     launch signature is out of date (an edited env, renewed credentials)
     or that a chat holds is moved to a detached key first. A spawn
     already running for the key is joined unless it started with another
     launch or a holder that excludes the caller. A spawn is the
     `spawnAgent` effect with the ensure's id (`ensure`, not serialized);
     its answer maps the agent under the key (a detached one when a newer
     launch took the key meanwhile), and a config deleted meanwhile kills
     it (`killAgent` with the same id; a failed kill is ignored). Every
     waiter judges a failure by its own options: a silent caller logs and
     gets no agent. A definite rejection is read from the message prefix
     (`ACP_AGENT_UNAVAILABLE: `, `ACP_INVALID_AGENT_CONFIG: `), so the app
     hands a coded web error back with that prefix. The Projects are
     `Book::projects`, which the app keeps equal to the project store
     (`setProjects`); the saved configs are `State::agent_configs`, set by
     `agentConfigsChanged` until the config slice is ported;
     `renewAgentCredentials` bumps a Project's credential epoch. Not
     exercised yet: the `chat` and `warm` holders and
     `surfaceDefiniteRejection`, which only the callers still to port pass.
   - 8c, warm pool (R6a-6, `acp/warm.rs`, `acp/setup_error.rs`).
     `prepareChat` gets the config's agent (`ensureLiveAgent`, warm
     holder) and opens an ephemeral, promotable session into the key's slot
     (`preparedSessions`, keyed by config, trimmed cwd and the MCP list's
     fingerprint), marking the key in `preparingChatKeys` meanwhile. A
     failure is classified for the launcher (`prepareChatErrors`, through
     a port of `classifySetupError` with its labels and detail text,
     checked against the TypeScript classifier's answers in
     `setup_error_tests.rs`); a spawn failure the user asked for toasts, and
     a background seed skips a key the host refused for good. A session
     whose agent died meanwhile is dropped, one whose prepare was
     cancelled or replaced is reaped, and one on an outdated launch is
     reaped and prepared again. `claimPreparedChat` answers the slot's
     session when it is live, on the current launch (`runsCurrentLaunch`,
     and the registry MCP servers it took, `registryMcpServersMatch`) and
     in the claiming Project; it promotes it (`promoteSession`, a failure
     toasts and keeps it ephemeral), saves the chat, clears the slot and
     seeds the next warm session for the selected config. A stale slot is
     dropped and its session reaped (`reapOrphanPreparedSession`: dispose,
     then delete; an indexed chat is never reaped). `cancelPreparedChat`
     reaps the slot's session unless it is the active chat. The ephemeral
     sessions (`Book::ephemeral_sessions`) are not chats to the agent
     holder check, and a close or disconnect drops one without a
     transcript. Not here: `startChat`, `startQuickChat`,
     `finalizeChatLaunch`, `applyPendingLauncherOptions`, the config slice,
     the promotion wait before a first prompt (`inFlightPromotions`) and
     `stoppingAgentIds`.
   - 8, rest. `startChat`,
     `startQuickChat`, `finalizeChatLaunch` and `applyPendingLauncherOptions`;
     the config slice. History open, import, the one-shots and the agent
     switch all wait on `ensureLiveAgent`.
9. In-chat agent switch.
   - 9a, markers (R6a-6, `acp/switch.rs`). `_onAgentSwitch` adds the
     separator record (`switch:seq-<seq>`, the event's seq or the next
     one) to `agentSwitches`, or updates the record a reconnect already
     installed with the same id or the same target and summary (it keeps
     its id, seq and arrival time); at most 500 stay. A covered event or a
     chat that takes no transcript events is dropped. The target's index
     row gets `continuedFrom` once, within the source's project, and moves
     to the front (`stampSwitchContinuation`); a history open heals the
     same edges from its payload (`healChainEdges`). The history watermark
     now counts switch markers (`maxPayloadSeq`). `cancelAgentSwitch`
     drops an armed switch.
   - 9b, arming (R6a-6). `armAgentSwitch` answers with `actionResolved`.
     The busy gate (`switchBlockedReason`) refuses while a turn runs, a
     restore or a launch is in flight, prompts are queued, a permission,
     question, elicitation or sign-in waits on the user, or background
     subagents or commands of the chat's own agent run. A refusal, the
     agent that already owns the chat, and an unknown config each stamp
     `Could not switch agent: …` on the chat. An armed chat gets
     `switching: { toConfigId, status: 'pending' }` and its target warms
     through `prepareChat` (silent). A chat with no `cwd` arms and then
     fails with `actionRejected`, where the TypeScript throws the engine's
     `TypeError` from `cwd.trim()` (a unit test, since the text differs per
     engine).
   - 9c, armed picks (R6a-7). `setSwitchPendingOption` merges a composer
     pick into `switching.pendingOptions` (a model pick keeps only the
     mode-category values, read from the target's warm session or else its
     options cache) and saves it under the target config, with the scoped
     values under the armed model (`modelValuesPatch`). Nothing goes to the
     chat's current agent. A pick with no switch armed is logged and dropped.
     Not here: the drop once `switchAgent` runs, which needs 9d.
   - 8d, launcher picks (R6a-7, `acp/launcher_options.rs`).
     `applyPendingLauncherOptions` sets a launcher's or an armed switch's
     picks on the new session, one set command at a time on a `Book::runs`
     id: the mode unless current, the model (native `setModel`, falling back
     to the model config option; a model the session lacks toasts and skips
     its scoped values), then each value the session advertises and does not
     already show. Each failure is logged and the next pick still goes. After
     a native model switch with values still to set it waits for the
     re-advertise (`waitForReadvertisedOptions`); the recorder cannot reach
     that wait, whose loop only ends on the mocked clock. Not here: the
     `restore` form the reopened chat's model restore uses.
     Not here: `switchAgent` (the handoff summary, the transcript splice, the
     new session and the first prompt) and the reopen's switch redirect and
     re-splice.
10. MCP, import, discovery, one-shot collectors.
    - 10a (R6a-4). The MCP slice. Every disk, probe and OAuth call is an
      effect (`loadMcpServers`, `saveMcpServers`, `probeMcpServer`,
      `startMcpOAuth`, `completeMcpOAuth`, `hasMcpOAuthToken`,
      `disconnectMcpOAuth`, `answerMcpInstall`) the app answers with
      `mcpCommandDone` or `mcpCommandFailed` (the raw message and an
      `McpOAuthError` code). Registry writes run one at a time through
      `Book::mcp`, as the TypeScript promise queue runs them, and a failed
      save puts the list back. A probe whose server was edited, deleted or
      signed in or out meanwhile does not land; it probes again. The first
      successful load starts the shared registry watch
      (`watchSharedMcpServers`, answered by `mcpSharedChanged` with the list
      the app normalized); `Book::mcp_shared_sync` remembers it, as the
      module flag does. An OAuth flow the model aborts (a newer flow, a
      cancel, a delete) ends as cancelled at once, after the step's own
      effects: the web rejects it that way, and the app's later answer is
      ignored. On the desktop, whose sign-in ignores the abort, the
      TypeScript logs the cancel only if the old flow later fails. The
      registry normalization and the legacy canvas entry migration stay in
      the app's persistence layer. Not here: the answering marks of
      `pending-permission-sync` (the resubscribe install, PR 6).
    - 10b, discovery (R6a-5, `acp/discovery.rs`). `discoverSessions` reads
      the agent's own sessions in a folder page by page: each page is a
      `listSessions` effect the app answers with `agentCommandDone` and the
      page, or `agentCommandFailed`. It runs only for a connected agent
      that advertises `sessionCapabilities.list`, once per agent and
      normalized folder (`discoveringKeys`), for at most 10 pages, and
      drops repeated session ids. An empty-string cursor is a page token;
      only a missing or null one ends the listing. A page refused for a
      sign-in goes through `withAuthRetry` (`session/list`); a failure
      clears the folder's old result, and a result whose agent
      disconnected meanwhile is dropped.
    - 10b, import (R6a-5, `acp/import.rs`). `listImportableSessions`
      lists through a live agent of the launch a new chat would get
      (`runsCurrentLaunch`), else through `ensureLiveAgent`: up to 200
      `listSessions` pages, ended early by a repeated cursor, keeping the
      sessions the index lacks. It answers `actionResolved` with
      `{ status, agentId, sessions }` (`unavailable` without an agent or
      after a disconnect, `unsupported` without `loadSession` and
      `session/list`). `importAgentSessions` sends one
      `importAgentSession` per session through the sign-in retry
      (`session/load`), reports each outcome as `importProgress`
      (`imported`, `duplicate` on the host's `duplicate` code, or
      `failed`), and stops before the next session after `stopImport`.
      Then it logs, emits its own `refreshSessionIndex`, and answers its
      outcomes once the app answers that effect with
      `sessionIndexReloaded` or `sessionIndexReloadFailed`; another index
      snapshot does not answer it. A listing answered with no page fails.
      `updatedAt` goes through a `Date.parse` port of the ISO forms; a time
      without an offset, which JavaScript reads in the local zone, is not
      parsed. The app names each call with a request id (the listing's
      fourth argument, the import request's `request`), and every
      `actionResolved` and `importProgress` it answers carries that id, so
      two listings or imports at once stay apart. The recorder has no such
      id; `import_tests.rs` checks it. A listing that fails still answers
      with a plain `actionRejected`.
    - 10c, one-shots (R6a-6, `acp/oneshot.rs`). `generateCommitMessage`,
      `assistTerminal` and `draftBugReport` check their input (the first
      two reject, the draft answers null), log their start, get the
      selected config's agent (`ensureLiveAgent`, `Caller::OneShot`), open
      an ephemeral session (`createSession`, which now answers either the
      user's action or a one-shot: `CreateCaller`), and send one prompt
      (`dispatchPrompt`). The session's collector takes the agent's text
      chunks, past a size cap fails; a tool call, a permission, question or
      elicitation request, an agent error or crash, a disconnect or a close
      fails it; `prompt_complete` ends it. None of those reach a transcript,
      and a disconnect drops the session outright. A collector that settles
      during an event moves its run on once the step ends, as the
      TypeScript promise chain runs after the handler. The reply is parsed
      (the commit summary and description, the assist text, the bug title
      and body), then the session is cancelled and disposed
      (`disposeEphemeralSession`) and its state dropped before the caller
      gets the answer. The overall time (60 s or 90 s) and the dispose time
      (2 s) are `startOneShotTimer` effects answered by `oneShotTimerDue`;
      a session that opens after its run ran out is disposed without an
      answer. The recorder never reaches a timer, so `oneshot_tests.rs`
      checks them. `State::selected_agent_config_id` stands in for the
      config slice's selection.
11. The user's own control picks (`setMode`, `setModel`, `setConfigOption`,
    `pickModel`), the saved-pick heal, and the prompt reconcile mapped onto
    the client crate's command ledger. Split in two:
    - 11a (R6a-2). `setMode`, `setModel` and `setConfigOption` as two steps,
      like sending: the action returns the set command, and the app hands
      the same effect back as `controlSet` once the agent acknowledged it,
      with the agent's option snapshot. The acknowledgement applies the
      value (the echo guard spares the option just set), updates the index
      row and the options cache, and saves the pick
      (`persistComposerOptions`, with the per-model values of a scoped
      option). Not yet: the re-auth retry (`withAuthRetry`, 7b), the pick
      marks a reopen restore reads (`noteUserPick`), and the drop of a
      warm-pool session's pick (PR 8).
    - 11b, first part (R6a-3). `pickModel` and the saved-pick heal
      (`reapplyPersistedComposerOptions`), as runs in `Book::runs`
      (`acp/restore.rs`). A run sends one command and waits: the record
      read (`readComposerOptions`, answered by `composerOptionsRead`), a set
      command's acknowledgement (`controlSet`) or failure (`controlFailed`),
      or, after a native model switch with values to restore, the agent's
      re-advertised options (`waitForReadvertisedOptions`, 1 s, answered by
      `readvertiseWaitOver` or by an event that changes the options). A
      newer pick stops an older one at its next step; a heal is skipped
      while a pick runs, and a trigger during a heal runs it once more after.
      The recorder's clock stands still within a step, so the wait, an
      interleaved read and a value moved during the read are Rust unit tests
      (`restore_tests.rs`), not vectors. Not here: the reopen restore
      (`reopenRestores`, `noteUserPick`; it needs history open, 6b) and the
      re-auth retry (`withAuthRetry`, 7b).
    - 11b, rest. The reopen restore and the prompt reconcile.
    - 11c, the prompt reconcile (R6a-6, `acp/reconcile.rs`). A dispatch the
      app settles as `uncertain` (the reply was lost, or the host answered
      `outcome_unknown`) keeps its turn open and waits in
      `promptCommandStates`, unless the turn moved on meanwhile or its echo
      already arrived. It is resent with the same turn id on the engine's
      ladder (`promptResendAfter`, 2, 4, 8, then every 15 s), on
      `resendPendingPrompts` when the transport recovers, and on
      `recheckPromptCommand`; a `transportReconnecting` flag holds the
      ladder, and an `outcome_unknown` hold goes out only on the user's
      check. The `user_prompt` echo (`echoedWirePromptTurns`) and the turn's
      end clear it; a rejected resend fails the turn. Stop abandons it: the
      turn id counts as ended, the turn closes, and a resend still on the
      wire cancels again once it settles. The ladder's timers, a reconnect
      and a resend in flight at Stop are Rust unit tests
      (`reconcile_tests.rs`). Not here: `reconcilePromptAfterRecovery`, which
      reads a recovered event log the Rune client does not hand the model.

### Sending as two steps

The TypeScript `runPromptTurn` awaits the dispatch inside one async
function. Two shapes were weighed.

- **Outcome events (chosen).** `sendPrompt` stages the turn and returns
  `dispatchPrompt { turnId, blocks }`. The app sends it through the client
  crate's ledger and answers with `promptSettled { turnId, outcome }`. What
  the outcome needs (the caller, the queued item, the turn it replaced) waits
  in `Book::dispatches`. Timers work the same way: `retryQueueAfter` comes
  back as `queueRetryDue`, and a send-now that stopped a turn resumes in the
  turn end that clears it.
- **A model-owned command ledger.** The model would keep resend timers and
  command states itself. Rejected: the client crate already resends a
  ledgered command with the same `commandId` across socket drops (AD-5), so
  a second ledger in the model would race it. The prompt reconcile
  (`slices/prompt-reconcile.ts`, 11c) starts only where that ledger stops:
  the app settles a dispatch as `uncertain` when the ledger gives up on a
  reply it cannot get, and the model's resends reuse the turn id, which the
  host dedups.

The vectors replay this the way the recorder ran it: each `dispatchPrompt`
settles with the next outcome the vector's `context.dispatch` lists, before
the deferred turn end. The recorder cannot see the busy-retry `setTimeout`,
so the replay drops `retryQueueAfter` before it compares.

## Not ported yet, on purpose

- `wireBlocksToDisplay` on the `user_prompt` echo. It reverses the composer's
  wire framing (skill paths, file tokens, conversation envelopes) and belongs
  to the composer unit, which ports `lib/composer/`. The model keeps the
  echo's blocks as sent; the vectors use prompts without framing.
- `handoffOnlyTurnIds` on the echo. PR 9.
- On `prompt_complete`: the update of the in-memory payload cache (PR 6).
- In sending: the warm-pool promotion
  wait and the launch's `skipUserAppend` and `leadsQueue` (PR 8), the
  history-open wait (`openingHistoryIds`, PR 6), the read-only guard from the
  chat's GitHub link (the model reads it from the transcript only), the
  re-auth after an expired token, the armed agent switch (PR 9), and
  send-now's 5 s wait timeout.

On a closed session the model does not yet run the reload hand-off
(`reloadInFlight`, PR 9) or the drop of an empty warm-pool session
(`ephemeralSessionIds`, PR 8).

## CI

`.github/workflows/parity-vectors.yml` records every store's vectors on a PR
that touches `src/renderer/**`, `src/shared/**`, the lockfile or the vectors,
and fails when `out/` is stale. A renderer change that changes what a store
does fails its own PR there; the re-recorded files then trigger the Rune
workflow, whose replay fails until the Rust follows. The Rune workflow
also runs on a PR that changes a pinned source, so its staleness test fails
on that PR and not on a later, unrelated Rune PR. Each recorder checks that
its pinned sources match a path glob of both workflows.

## Recorder limits

- The TypeScript clamp log remembers ids across vectors, and only a stored
  call's id can be forgotten from outside. Vectors that log a clamp use ids
  no other vector uses.
- The recorder's transport is a stub whose `historyMode` is `tauri_store`
  (the desktop) or, with `context.serverHistory`, `server`. A relay
  (`live_only`) client skips the index reload the way the server one does;
  the model folds both into `Env::server_history`.
- `context.openChatTabs` lists the sessions a chat tab shows.
- `context.desktop` makes `isTauriContext` true, as `Book::desktop` does.
- `context.payloads` is each session's durable history: every
  `loadSessionPayload` emits `readSessionPayload` and answers it the way an
  agent command answers its reply (below), null for a session it does not
  list. The payload cache is cleared per vector.
- `context.mcpReplies` answers the MCP commands in order: an entry is the
  reply, `{ "reject": text, "code": c }` fails it (an `McpOAuthError` when
  `code` is set), `{ "hold": true }` leaves it in flight, and a start's
  `authUrl` reaches the paste-back callback first. A held start rejects as
  cancelled when its signal aborts. The MCP fields are snapshotted only once
  a vector set one. The store's OAuth flows and its shared-watch flag are
  module state no reset reaches, so a vector that leaves a flow in flight
  uses a server id no other vector does, and every vector after the first
  load starts with `book.mcpSharedSync`.
- Logs without a level record the `error` that `logFrontendError` defaults
  to.
- Vectors over the live bounds keep their records small: every step
  snapshots the whole state.
- Toasts are effects (`toast`, with the level). The roster watch is attached
  once to a stub watch client; `agentsWatchState` and `agentsWatchDelta` call
  its handlers.
- `context.agentReplies` answers the agent commands and the close and
  delete commands in order: an entry is the reply, `{ "reject": text }`
  fails it, and `{ "hold": true }` leaves it in flight for the rest of the
  vector. `{ "hold": true, "release": reply }` holds it until a
  `releaseHeld` step, which answers the oldest held command with `reply`;
  that is how a vector interleaves a close with a delete, or a durability
  probe with a close.
- `context.projects` is the project store's list; a `setProjects` step
  replaces it, `agentConfigsChanged` replaces `agentConfigs`, and
  `renewAgentCredentials` calls the module function. The spawn bookkeeping
  (`inFlightWarms`, the definite rejections, the signatures) is reset per
  vector.
- An action whose answer its caller reads (`listImportableSessions`,
  `importAgentSessions`) records it as `actionResolved`. The recorder's
  index reload never settles, so an import's answer is not recorded;
  `import_tests.rs` checks it. The import's progress callback is
  `importProgress`, and a `stopImport` step makes its stop poll true. An
  `agentReplies` entry `{ "reject": text, "code": c }` fails with an
  `AcpTransportError` of that code.
- `context.setReplies` answers `setMode` and `setModel` in order, as the
  agent commands do; `context.factoryKey` is the stored-key answer (`true`,
  `false`, or `{ "reject": text }`).
- The remembered-auth writes (`persistAuthMethods`) go through a promise
  chain, so where one lands among a step's other effects depends on the
  chain's queued microtasks. The replay compares them after the other
  effects, in their own order.
- `context.composerOptions` is the saved record every `readComposerOptions`
  answers (null by default). A `context.configReplies` entry
  `{ "reject": text }` fails that `setConfigOption` with `text`.
- An `agent_error` or `agent_crashed` without a `message` leaves the chat's
  `lastError` absent (`undefined`), not null; `Session::last_error` keeps
  the three states. An `agent_disconnected` without a `reason` counts as a
  deliberate end.
- The host's verdict on a failed command (`acp/host_error.rs`). `/ws`
  sends it as the reply's `code` and drops the message's tag; the desktop
  keeps the tag (`ACP_AGENT_UNAVAILABLE: …`) and sends no code. So
  `agentCommandFailed` and `controlFailed` carry the code, and the
  definite-rejection, signed-out, auth-required, provider and ambiguous
  checks read both, in `classifySetupError`'s order. A rejected action
  whose error had a code prints as `AcpTransportError: …`, the way
  `String(error)` prints it. An `agentReplies` or `setReplies` entry
  `{ "reject": text, "code": c }` records a `/ws`-shaped error.
- A failed command comes back with the error's raw message
  (`controlFailed`, `agentCommandFailed`, a failed `promptSettled`); the
  model adds the `Error: ` prefix `String(error)` prints wherever the store
  shows or keeps it.
- A history open's replay window closes on a timer the recorder's step
  does not wait for, so its effects land in the next step, after that
  step's own commands; the replay carries `endReplay` over the same way and
  drops it from the compared effects. The history vectors end with a no-op
  step (`clearPendingSignIn`) for that. An action that resolves with no
  value records `actionResolved` with `null`. The in-flight history opens
  are reset per vector.
- The TypeScript seq counter only moves up, so each vector starts on the next
  hundred and records the start in `book.seq`. Adding a scenario shifts the
  seqs of the ones after it; re-record and commit the whole file.

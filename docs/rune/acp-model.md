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
| `slices/mcp.ts` | 682 | `acp/mcp.rs` | state; probe, OAuth and writes are effects | 10 |
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
     it probe again where the TypeScript would.
   - 6b, rest. History open, `loadOlderMessages` (it needs the composer
     unit's wire-to-display reversal for its twin folding and the switch
     bands of PR 9), the resubscribe install, live-turn reattach.
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
9. In-chat agent switch.
10. MCP, import, discovery, one-shot collectors.
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
  (`slices/prompt-reconcile.ts`) maps onto that ledger in PR 11.

The vectors replay this the way the recorder ran it: each `dispatchPrompt`
settles with the next outcome the vector's `context.dispatch` lists, before
the deferred turn end. The recorder cannot see the busy-retry `setTimeout`,
so the replay drops `retryQueueAfter` before it compares.

## Not ported yet, on purpose

- `wireBlocksToDisplay` on the `user_prompt` echo. It reverses the composer's
  wire framing (skill paths, file tokens, conversation envelopes) and belongs
  to the composer unit, which ports `lib/composer/`. The model keeps the
  echo's blocks as sent; the vectors use prompts without framing.
- The prompt-reconcile bookkeeping on the echo (`echoedWirePromptTurns`,
  `clearPromptReconcileForTurn`) and `handoffOnlyTurnIds`. PR 11 and PR 9.
- The one-shot collectors that swallow chunks, tool calls and requests of
  commit-message, terminal assist and bug-report sessions. PR 10.
- Switch markers in `maxPayloadSeq`. PR 9.
- On `prompt_complete`: the prompt-reconcile clear (PR 11), and the update of
  the in-memory payload cache (PR 6).
- In sending: an outcome-unknown dispatch (PR 11), the warm-pool promotion
  wait and the launch's `skipUserAppend` and `leadsQueue` (PR 8), the
  history-open wait (`openingHistoryIds`, PR 6), the read-only guard from the
  chat's GitHub link (the model reads it from the transcript only), the
  re-auth after an expired token, the armed agent switch (PR 9), and
  send-now's 5 s wait timeout.

On a closed session the model does not yet run the reload hand-off
(`reloadInFlight`, PR 9), the one-shot collectors (PR 10) or the drop of an
empty warm-pool session (`ephemeralSessionIds`, PR 8).

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
  `loadSessionPayload` emits `readSessionPayload` and answers it, null for a
  session it does not list, and `{ "reject": text }` fails the read. The
  payload cache is cleared per vector.
- Vectors over the live bounds keep their records small: every step
  snapshots the whole state.
- Toasts are effects (`toast`, with the level). The roster watch is attached
  once to a stub watch client; `agentsWatchState` and `agentsWatchDelta` call
  its handlers.
- `context.agentReplies` answers the agent commands in order: an entry is
  the reply, `{ "reject": text }` fails it, and `{ "hold": true }` leaves it
  in flight for the rest of the vector.
- `context.composerOptions` is the saved record every `readComposerOptions`
  answers (null by default). A `context.configReplies` entry
  `{ "reject": text }` fails that `setConfigOption` with `Error: text`.
- The TypeScript seq counter only moves up, so each vector starts on the next
  hundred and records the start in `book.seq`. Adding a scenario shifts the
  seqs of the ones after it; re-record and commit the whole file.

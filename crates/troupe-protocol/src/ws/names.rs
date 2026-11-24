//! The closed name sets of `/ws`: request types, event types, watch names,
//! error codes and delivery tiers.

wire_enum! {
    /// A `/ws` request `type`.
    pub enum WsRequestType {
        SendPrompt = "send_prompt",
        CancelPrompt = "cancel_prompt",
        // Stop one subagent of a chat (`session/cancel` for its child session).
        StopSubagent = "stop_subagent",
        SetConfigOption = "set_config_option",
        SetMode = "set_mode",
        SetModel = "set_model",
        RespondPermission = "respond_permission",
        AnswerQuestion = "answer_question",
        // Chat approval of an MCP install. The payload may carry secret field
        // values, so the host does not ledger it.
        AnswerMcpInstall = "answer_mcp_install",
        // Stop agent browsing for a session (desktop parity with the
        // `browser_stop` Tauri command); only an authenticated client of the
        // session may send it.
        BrowserStop = "browser_stop",
        CreateSession = "create_session",
        LoadSession = "load_session",
        ResumeSession = "resume_session",
        CloseSession = "close_session",
        DisposeEphemeralSession = "dispose_ephemeral_session",
        // Story 8: promote a backend-ephemeral warm-pool session to durable
        // (registers persistence metadata + clears the ephemeral mark).
        PromoteSession = "promote_session",
        ListSessions = "list_sessions",
        RegisterDiscoveredSession = "register_discovered_session",
        ImportAgentSession = "import_agent_session",
        SpawnAgent = "spawn_agent",
        KillAgent = "kill_agent",
        ListAgents = "list_agents",
        // WS connection token-gate handshake (pre-auth). Distinct from
        // `authenticate_agent` (the ACP agent method).
        Authenticate = "authenticate",
        // ACP agent-advertised `authenticate` method (e.g. `pi_terminal_login`).
        // Post-auth request routed to `AcpManager::authenticate` on the host.
        AuthenticateAgent = "authenticate_agent",
        LogoutAgent = "logout_agent",
        DeleteAgentSession = "delete_agent_session",
        RespondElicitation = "respond_elicitation",
        Subscribe = "subscribe",
        Ping = "ping",
        ListPersistedSessions = "list_persisted_sessions",
        // CAP-11: host-owned session delete (desktop parity with the
        // `acp_history_delete` Tauri command). Boolean reply (finding 6): `true` =
        // deleted, `false` = record already absent (idempotent no-op); genuine
        // errors reject. Publishes a `chat_history` watch delta on a real delete;
        // live-only mode → `unsupported`.
        DeleteSession = "delete_session",
        // Quick chat local rename. Sets TitleSource::LocalAlias. Quick chats only.
        RenameSession = "rename_session",
        OpenPersistedSession = "open_persisted_session",
        GetSessionPayload = "get_session_payload",
        RecoverSessionSnapshot = "recover_session_snapshot",
        // R2: lightweight server-authoritative replay cursor (no snapshot payload).
        // Unlike `recover_session_snapshot` (which re-registers a subscription),
        // this only returns the durable `{ sessionId, watermark }` (plus the log's
        // `logEpoch`/`headSeq`, see `SessionCursorReply`) so a refreshed transport
        // can seed its `(logEpoch, lastSeq)` cursor before its first subscribe.
        GetSessionCursor = "get_session_cursor",
        // Tail-first variant of `get_session_payload`: fetches only the last `limit`
        // messages + matching tool calls so the renderer can install the recent
        // transcript immediately and lazy-load the full payload on scroll-up.
        GetSessionPayloadTail = "get_session_payload_tail",
        // CAP-6 / Story 8: host-owned ACP catalog resolution. The web client
        // resolves the catalog through `list_acp_catalog` (the host's OS/arch/
        // runtime + per-agent `SupportedAcpAgentStatus`) + `set_catalog_opt_in`
        // (the host-persisted opt-in that gates CDN registry augmentation). The
        // web client never probes `@tauri-apps/plugin-os` or PATH locally — the
        // host is the single source of truth.
        ListAcpCatalog = "list_acp_catalog",
        SetCatalogOptIn = "set_catalog_opt_in",
        // The registry check behind "Check for updates": `{ reason: 'manual' |
        // 'background' }`, answered with the host's `AcpRegistrySnapshot`
        // (`source: 'cache'` when the registry was unreachable).
        FetchAcpRegistrySnapshot = "fetch_acp_registry_snapshot",
        // MCP catalog (spine AD-1): `{ query?, cursor?, limit? }` → one
        // `McpCatalogPage` of the host's converted registry snapshot. A read.
        SearchMcpCatalog = "search_mcp_catalog",
        GetMcpCatalogStatus = "get_mcp_catalog_status",
        SetMcpCatalogSync = "set_mcp_catalog_sync",
        DescribeMcpInstall = "describe_mcp_install",
        PlanMcpInstall = "plan_mcp_install",
        CommitMcpInstall = "commit_mcp_install",
        CancelMcpInstall = "cancel_mcp_install",
        GetMcpCatalogIcon = "get_mcp_catalog_icon",
        // CAP-6 / Story 9: host-owned verified-atomic ACP install. The web client
        // installs a catalog agent through `install_acp_agent` (the host downloads +
        // verifies sha256 + extracts + atomically activates). The request is
        // `{ agentId }` only; the host resolves everything from the trusted catalog.
        InstallAcpAgent = "install_acp_agent",
        // Headless ACP auth (spec-acp-terminal-auth): deliver a user-pasted
        // loopback OAuth redirect URL to the agent's callback listener on the
        // host. Payload `{ agentId, url }`; reply `{ status }` (the replay's HTTP
        // status). The host validates http(s) + loopback-only before fetching
        // (SSRF guard). NOTE: this request keeps the `acp_` prefix — the wire
        // name is fixed by the contract, not the prefix-drop convention.
        AcpDeliverAuthRedirect = "acp_deliver_auth_redirect",
        // Issue #613: server-side generic key-value store — the web client routes
        // its `persistenceApi` through these (replacing the per-browser localStorage
        // stub) so settings / layout / command history / SSH profiles survive
        // browser switches + server restarts. Keys are scoped by prefix
        // (`device/<deviceId>/…`, `shared/lww/…`, `shared/cas/…`) and every record
        // carries a revision + the writing `clientInstanceId` (issue #1073). Errors
        // carry SCREAMING_SNAKE_CASE codes via `err_with_code`: `STORE_UNAVAILABLE`
        // (no store attached / unreadable), `STORE_WRITE_FAILED` /
        // `STORE_DELETE_FAILED` (IO), `VALIDATION_ERROR` (malformed payload,
        // empty/whitespace key, key > 1024 bytes — CAP-11), `STORE_VALUE_TOO_LARGE`
        // (serialized value > 256 KiB — CAP-11), `STORE_KEY_SCOPE` (key outside the
        // three scopes or another device's partition), `STORE_BASE_REQUIRED`
        // (shared/lww write to an existing key without `baseRevision`),
        // `STORE_CAS_FAILED` (shared/cas `expected` mismatch), `STORE_PARTITION_FULL`
        // (the device partition would exceed its byte budget).
        StoreRead = "store_read",
        StoreWrite = "store_write",
        StoreDelete = "store_delete",
        // Project registry commands (one host command service shared with the HTTP
        // routes `POST /projects`, `PUT /projects/{id}`, `DELETE /projects/{id}`).
        // They persist to the host's durable registry, then publish the new state to
        // every `projects` watcher. Add / update / remove are idempotent by
        // `projectId`; a remove or archive with live sessions/PTYs is refused
        // `project_busy` (`err.details` lists them) unless the request carries
        // `cascade: true`.
        AddProject = "add_project",
        UpdateProject = "update_project",
        RemoveProject = "remove_project",
        // CAP-2 (spec-in-chat-agent-switch): host-authored durable agent-switch
        // marker: the host
        // numbers ONE `agent_switch` record, appends it durably and delivers the
        // `acp:agent_switch` event to live clients under the same seq. Reply `{}`;
        // unknown session → `not_found`; live-only mode → `unsupported`.
        RecordAgentSwitch = "record_agent_switch",
        // A surface read a chat: `{ sessionId, seq, logEpoch? }` (the session-log
        // seq of the newest turn end it showed, and the log epoch it belongs to) →
        // `{ sessionId, seq, logEpoch }`, the host's shared last-read marker as it
        // stands. Idempotent and monotonic: an older seq, or one from another log
        // epoch, is a no-op. The chat's `chat_activity` record follows the marker.
        // Unknown session → `not_found`.
        MarkChatRead = "mark_chat_read",
        // Watches `{ watch, scope? }` (`projects`, `chat_history`, `agents`; `fs`
        // and `terminals` scoped to a project id; `store` scoped to a key): `watch`
        // queues the full-state frame, then replies `{ watch, scope, revision }`;
        // later changes arrive as deltas under `n + 1`. A second `watch` of the same
        // (watch, scope) replaces the subscription and sends fresh state. `unwatch`
        // is idempotent. Unknown watch or scope → `unsupported`; an `fs` watch of an
        // unknown/archived project → `not_found`; a `store` watch on a host without
        // a store → `STORE_UNAVAILABLE`.
        Watch = "watch",
        Unwatch = "unwatch",
        // Explicit host-default project change (updates `default_project_id`,
        // persists on file-backed hosts, publishes the project state to every
        // `projects` watcher).
        SetDefaultProject = "set_default_project",
        // Orchestrated sub-agents (acp-subagent-orchestrator): `get` reads the
        // settings view `{ enabled, maxRunning, maxDepth, agents: [{ agentId,
        // name, allowed, model, description }] }`; `set` writes a partial of it (an omitted field keeps
        // its value) and answers the new view. One host engine serves every chat.
        SubagentSettingsGet = "subagent_settings_get",
        SubagentSettingsSet = "subagent_settings_set",
        // Claude Accounts (ADR-0004): list `{}` → `ClaudeAccountsSnapshot`; choose
        // the Default `{ accountId }` → `{}` or a Project Account `{ projectId,
        // accountId | null, expectedAccountId? }` → `{ applied }`, where
        // `expectedAccountId` makes the choice a compare-and-set.
        ListClaudeAccounts = "list_claude_accounts",
        // `{ force? }` → `{ usage: ClaudeAccountUsage[] }`; reads are cached 5 min
        // on the host (a forced read still reuses one younger than 60 s).
        ListClaudeUsage = "list_claude_usage",
        SetDefaultClaudeAccount = "set_default_claude_account",
        SetProjectClaudeAccount = "set_project_claude_account",
        // `{ projectId, sessionId, accountId | null, expectedAccountId? }` →
        // `{ applied }`: the chat's own Account; `null` follows the Project again.
        // `expectedAccountId` compares with the Account the chat resolves to now.
        SetChatClaudeAccount = "set_chat_claude_account",
        // `{ enabled }` → `{}`: host-wide automatic switching at a usage limit.
        SetClaudeAutoSwitch = "set_claude_auto_switch",
        // `{ sessionId, at }` → `{}`: the host continues the chat at `at` (epoch
        // ms) after its Claude limit resets; `at: null` cancels.
        SetClaudeResume = "set_claude_resume",
        // `{ name }` → the new signed-out `ClaudeAccount`.
        AddClaudeAccount = "add_claude_account",
        // `{ accountId, name }` → `{}`.
        RenameClaudeAccount = "rename_claude_account",
        // `{ accountId }` → `RemovedClaudeAccount` (`{ movedProjectIds,
        // movedChatIds }`): the Projects that moved to the Default Account and the
        // chats that had the Account as their own; empty when the Account was
        // already gone. The built-in Account is refused.
        RemoveClaudeAccount = "remove_claude_account",
        // Link sign-in: `start` `{ accountId }` → `{ url }` (the Claude sign-in
        // link; replaces a running sign-in of the Account). `finish` `{ accountId,
        // code }` → `{}` once the CLI stored the login; `claude_login_code_incomplete`
        // and `claude_login_busy` keep the sign-in running, any other error ended it.
        // `cancel` `{ accountId }` →
        // `{ cancelled }` stops a browser or link sign-in.
        StartClaudeLinkLogin = "start_claude_link_login",
        FinishClaudeLinkLogin = "finish_claude_link_login",
        CancelClaudeLogin = "cancel_claude_login",
        // Codex Accounts. ChatGPT logins only. `list` → `CodexAccountsSnapshot`.
        // Choose the Default `{ accountId }` → `{}` or a Project `{ projectId,
        // accountId | null, expectedAccountId? }` → `{ applied }`.
        ListCodexAccounts = "list_codex_accounts",
        // `{ force? }` → `{ usage: CodexAccountUsage[] }`. Cached 5 min on the host.
        // A forced read still reuses one younger than 60 s.
        ListCodexUsage = "list_codex_usage",
        SetDefaultCodexAccount = "set_default_codex_account",
        SetProjectCodexAccount = "set_project_codex_account",
        // `{ projectId, sessionId, accountId | null, expectedAccountId? }` →
        // `{ applied }`: the chat's own Account; `null` follows the Project again.
        SetChatCodexAccount = "set_chat_codex_account",
        // `{ enabled }` → `{}`. A chat at its Account's limit continues on the
        // signed-in Account with the most room.
        SetCodexAutoSwitch = "set_codex_auto_switch",
        // `{ sessionId, at }` → `{}`: the host continues the chat at `at` (epoch
        // ms) after its Codex limit resets; `at: null` cancels.
        SetCodexResume = "set_codex_resume",
        // `{ name }` → the new signed-out account.
        AddCodexAccount = "add_codex_account",
        // `{ accountId, name }` → `{}`.
        RenameCodexAccount = "rename_codex_account",
        // `{ accountId }` → `{ movedProjectIds }`. Runs `codex logout` for that home
        // first. The built-in Account is refused. An Account already gone answers
        // `{ movedProjectIds: [] }`.
        RemoveCodexAccount = "remove_codex_account",
        // `{ accountId, config }` → the spawn outcome of an agent whose home is that
        // Account. The client then calls `authenticate_agent` with `chat-gpt` or
        // `chat-gpt-device-code`. The config env is not stored in the ledger.
        StartCodexAccountLogin = "start_codex_account_login",
        // Antigravity Accounts. Google logins only; the same shapes as the Codex
        // Accounts with `AntigravityAccountsSnapshot` and `AntigravityAccountUsage`
        // (one `model` window per quota bucket). `list` → the snapshot.
        ListAntigravityAccounts = "list_antigravity_accounts",
        // `{ force? }` → `{ usage: AntigravityAccountUsage[] }`. Cached 5 min on
        // the host. A forced read still reuses one younger than 60 s.
        ListAntigravityUsage = "list_antigravity_usage",
        SetDefaultAntigravityAccount = "set_default_antigravity_account",
        SetProjectAntigravityAccount = "set_project_antigravity_account",
        // `{ projectId, sessionId, accountId | null, expectedAccountId? }` →
        // `{ applied }`.
        SetChatAntigravityAccount = "set_chat_antigravity_account",
        // `{ enabled }` → `{}`.
        SetAntigravityAutoSwitch = "set_antigravity_auto_switch",
        // `{ sessionId, at }` → `{}`; `at: null` cancels.
        SetAntigravityResume = "set_antigravity_resume",
        // `{ name }` → the new signed-out account.
        AddAntigravityAccount = "add_antigravity_account",
        // `{ accountId, name }` → `{}`.
        RenameAntigravityAccount = "rename_antigravity_account",
        // `{ accountId }` → `{ movedProjectIds, movedChatIds }`. Deletes that
        // home's login files first. The built-in Account is refused.
        RemoveAntigravityAccount = "remove_antigravity_account",
        // `{ accountId, config }` → the spawn outcome of an agent whose
        // `GEMINI_HOME` is that Account. The client then calls
        // `authenticate_agent` with `oauth-personal` or `oauth-business`.
        StartAntigravityAccountLogin = "start_antigravity_account_login",
        // The canvas gateway's renderer leg (spine AD-21, AD-29). `canvas_mount`
        // / `canvas_unmount` `{ projectId, mountSeq }` tell the host this connection
        // holds (or no longer holds) a project's canvas; the host then forwards
        // agent canvas calls to it as top-level `canvas_call` frames, answered with
        // `canvas_reply { callId, mountSeq, ok, result | error }`. `params` and
        // `result` are JSON text, never parsed on the way. Replies `{}`,
        // `{ unmounted }` and `{ resolved }`. A long `result` goes in several
        // `canvas_reply`, each `partial: true` but the last, answered `{ accepted }`.
        CanvasMount = "canvas_mount",
        CanvasUnmount = "canvas_unmount",
        CanvasReply = "canvas_reply",
    }
}

wire_enum! {
    /// The `type` of a `/ws` session log event. `gap` and `heartbeat` name the
    /// top-level delivery frames. The host publishes every session-scoped ACP
    /// event, so this list also holds the nine types after `heartbeat` that the
    /// hand-written `WS_EVENT_TYPES` lacks.
    pub enum WsEventType {
        AgentSpawned = "agent_spawned",
        SessionCreated = "session_created",
        SessionClosed = "session_closed",
        AgentDisconnected = "agent_disconnected",
        AgentError = "agent_error",
        ToolCall = "tool_call",
        ModeUpdate = "mode_update",
        ConfigOptionsUpdate = "config_options_update",
        SessionInfoUpdate = "session_info_update",
        UsageUpdate = "usage_update",
        PermissionRequest = "permission_request",
        PromptComplete = "prompt_complete",
        MessageChunk = "message_chunk",
        ToolCallUpdate = "tool_call_update",
        CommandsUpdate = "commands_update",
        PlanUpdate = "plan_update",
        AuthRequired = "auth_required",
        UserPrompt = "user_prompt",
        AgentSwitch = "agent_switch",
        LocalTitleGenerated = "local_title_generated",
        AgentWorking = "agent_working",
        BackgroundCommands = "background_commands",
        Gap = "gap",
        Heartbeat = "heartbeat",
        QuestionRequest = "question_request",
        QuestionCancelled = "question_cancelled",
        ElicitationRequest = "elicitation_request",
        McpInstallRequest = "mcp_install_request",
        McpInstallResolved = "mcp_install_resolved",
        McpRegistryChanged = "mcp_registry_changed",
        BrowserTicketClosed = "browser_ticket_closed",
        ClaudeLimitAction = "claude_limit_action",
        CodexLimitAction = "codex_limit_action",
        AntigravityLimitAction = "antigravity_limit_action",
    }
}

wire_enum! {
    /// A watch a connection can hold. `projects`, `chat_history`, `agents` and
    /// `chat_activity` have the empty scope; `fs` and `terminals` are scoped
    /// to a project id, `store` to a store key.
    pub enum WsWatchName {
        Projects = "projects",
        ChatHistory = "chat_history",
        Agents = "agents",
        Fs = "fs",
        Store = "store",
        Terminals = "terminals",
        ChatActivity = "chat_activity",
    }
}

wire_enum! {
    /// The stable snake_case `err.code` strings of `/ws`. Some handlers
    /// (the store, installs) answer SCREAMING_SNAKE_CASE codes outside this
    /// set, so `WsError::code` stays a string.
    pub enum WsErrorCode {
        NotFound = "not_found",
        Unauthorized = "unauthorized",
        RateLimited = "rate_limited",
        AgentCrashed = "agent_crashed",
        PermissionDenied = "permission_denied",
        Stale = "stale",
        Duplicate = "duplicate",
        Unsupported = "unsupported",
        NotImplemented = "not_implemented",
        AgentAuthRequired = "agent_auth_required",
        SessionOwnedByOther = "session_owned_by_other",
        ProtocolVersionMismatch = "protocol_version_mismatch",
        InvalidClientIdentity = "invalid_client_identity",
        TerminalLimit = "terminal_limit",
        ProjectBusy = "project_busy",
        OutcomeUnknown = "outcome_unknown",
        DestructiveOpBlocked = "destructive_op_blocked",
        ProviderQuotaExceeded = "provider_quota_exceeded",
        ProviderSessionLimit = "provider_session_limit",
        ProviderRequestRejected = "provider_request_rejected",
        Forbidden = "forbidden",
        AlreadyResolved = "already_resolved",
        AgentUnavailable = "agent_unavailable",
        InvalidAgentConfig = "invalid_agent_config",
        CommandConflict = "command_conflict",
        InvalidCommandId = "invalid_command_id",
        ClaudeAccountSignedOut = "claude_account_signed_out",
        CodexAccountSignedOut = "codex_account_signed_out",
        AntigravityAccountSignedOut = "antigravity_account_signed_out",
        ClaudeLoginCodeIncomplete = "claude_login_code_incomplete",
        ClaudeLoginBusy = "claude_login_busy",
    }
}

wire_enum! {
    /// How the relay delivers an event type to a slow client.
    pub enum ReliabilityTier {
        Lossy = "lossy",
        Reliable = "reliable",
        Idempotent = "idempotent",
    }
}

wire_enum! {
    /// Whether the host keeps session history.
    pub enum HistoryMode {
        Server = "server",
        LiveOnly = "live_only",
    }
}

impl WsEventType {
    /// The delivery tier: lossy streams are dropped for a slow client and
    /// followed by a `gap` frame, `prompt_complete` is deduplicated by turn,
    /// everything else is never dropped silently.
    #[must_use]
    pub const fn tier(self) -> ReliabilityTier {
        match self {
            Self::MessageChunk | Self::ToolCallUpdate | Self::CommandsUpdate | Self::PlanUpdate => {
                ReliabilityTier::Lossy
            }
            Self::PromptComplete => ReliabilityTier::Idempotent,
            _ => ReliabilityTier::Reliable,
        }
    }
}

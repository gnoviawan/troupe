# ADR-0006: Quick chats use a host-owned folder and an explicit kind

| Field | Value |
|-------|-------|
| Status | Accepted |
| Date | 2026-11-15 |

## Context

A quick chat is an agent chat with no project. The session still needs a working directory, because every agent starts with a `cwd`. That directory must not be a path the client chooses, and it must not be attached to a project by the registry.

The existing unassigned-project sentinel (`__unassigned__`) means "no project could be resolved." Older sessions already carry it. Reusing it for quick chats would show those older sessions in the new list.

The agent assigns the session id only after `session/new` returns. The process must already be running inside the quick-chat folder, so the folder name cannot be that session id. The host mints a folder id first.

## Decision

- A session record and its index entry carry `kind`. `quick` is a quick chat. The field is omitted for a project chat. Old records load as project chats.
- The host creates `~/.troupe/quick-chats/<id>` when a quick chat is created. `<id>` is a host-minted single path segment. The client does not send a cwd.
- Resume, load, and `list_sessions` accept that folder on desktop IPC and on `/ws` only after the path is canonicalized. A `..` climb or a symlink that leaves the root is refused.
- `find_by_path` does not return a project for a path inside the quick-chat root.
- A user rename of a quick chat is stored as `TitleSource::LocalAlias`. Project chats cannot use that rename.
- Deleting a quick chat deletes the session and the folder. A symlink is removed as a link, not by following it.
- There is no setting for the root, and no move-to-project, in this version.

## Consequences

- Legacy unassigned sessions stay out of Quick chats.
- A quick chat has no project environment, so a project's `CODEX_HOME` does not apply. The Codex Accounts account does.
- The folder id and the agent session id are different. The index id remains the agent session id. The folder id is `quickChatId` on the host record.

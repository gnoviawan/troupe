//! Version and size limits shared by both sockets.

/// The `authenticate` protocol version this contract describes. Both sockets
/// refuse any other version with `protocol_version_mismatch`. Bump it on any
/// wire-contract change.
pub const WS_PROTOCOL_VERSION: u32 = 7;

/// Longest accepted `deviceId` / `clientInstanceId`, in bytes.
pub const MAX_CLIENT_ID_LEN: usize = 128;

/// Longest accepted `commandId`, in bytes. A well-formed one is 1-128
/// characters of `[A-Za-z0-9_-]`.
pub const MAX_COMMAND_ID_LEN: usize = 128;

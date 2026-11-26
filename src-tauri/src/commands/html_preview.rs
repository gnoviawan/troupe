//! HTML preview Tauri commands, the twins of `POST /html-preview/open|update|close`
//! in `web/html_preview_api.rs`. The desktop serves the preview pages on the
//! canvas loopback listener, so `open` answers with its `http://127.0.0.1:<port>`
//! base, or `BUNDLE_LISTENER_DOWN` while it is not bound.

use std::sync::Arc;

use tauri::State;

use super::IpcResult;
use crate::canvas::static_server::{StaticServer, CODE_BUNDLE_LISTENER_DOWN};
use crate::html_preview::{log_file_name, registry, PreviewOpenInfo};

const LISTENER_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// `html_preview_open { path }`: a preview session for the `.html`/`.htm`
/// file at the absolute `path`.
#[tauri::command]
pub async fn html_preview_open(
    path: String,
    server: State<'_, Arc<StaticServer>>,
) -> Result<IpcResult<PreviewOpenInfo>, String> {
    let file = log_file_name(&path);
    let Some(base) = server.wait_for_base(LISTENER_WAIT).await else {
        log::warn!("[html-preview] open failed file={file} code={CODE_BUNDLE_LISTENER_DOWN}");
        return Ok(IpcResult::error(
            "the preview listener is not running",
            CODE_BUNDLE_LISTENER_DOWN,
        ));
    };
    Ok(match registry().open(&path, base.base_url) {
        Ok(info) => {
            log::info!("[html-preview] open file={file}");
            IpcResult::success(info)
        }
        Err(err) => {
            log::warn!(
                "[html-preview] open refused file={file} code={}",
                err.code.as_str()
            );
            IpcResult::error(err.message, err.code.as_str())
        }
    })
}

/// `html_preview_update { token, content }`: serve the editor's unsaved text
/// in place of the session's file.
#[tauri::command]
pub async fn html_preview_update(token: String, content: String) -> Result<IpcResult<()>, String> {
    Ok(match registry().update(&token, content) {
        Ok(()) => IpcResult::success(()),
        Err(err) => {
            log::warn!("[html-preview] update refused code={}", err.code.as_str());
            IpcResult::error(err.message, err.code.as_str())
        }
    })
}

/// `html_preview_close { token }`: end the session. Idempotent.
#[tauri::command]
pub async fn html_preview_close(token: String) -> Result<IpcResult<()>, String> {
    let closed = registry().close(&token);
    log::info!("[html-preview] close closed={closed}");
    Ok(IpcResult::success(()))
}

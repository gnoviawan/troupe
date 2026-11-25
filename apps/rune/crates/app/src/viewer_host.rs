//! The viewers' effects on the host: a media tab on screen reads its file's bytes
//! (`GET /fs/raw`), and the answer opens the image, PDF or video.

use model::Level;
use ui::viewers::Effect;

use crate::State;

impl State {
    pub(crate) fn run_viewer_effects(&mut self) {
        for effect in std::mem::take(&mut self.viewers.effects) {
            match effect {
                Effect::Read { path } => match &mut self.host {
                    Some(host) => host.fetch_bytes(path),
                    None => self.viewers.loaded(&path, Err(("NO_SERVER".into(), "No server".into()))),
                },
                Effect::Log { level, message } => match level {
                    Level::Error => log::error!("[viewer] {message}"),
                    Level::Warn => log::warn!("[viewer] {message}"),
                    Level::Info => log::info!("[viewer] {message}"),
                },
            }
        }
    }

    /// Hands the bytes the host read to their viewers; true when one arrived.
    pub(crate) fn pump_viewers(&mut self) -> bool {
        let Some(host) = &mut self.host else {
            return false;
        };
        let read = host.media_fetched();
        let changed = !read.is_empty();
        for (path, result) in read {
            let result = result.map_err(|e| match e {
                client::http::HttpError::Refused { code, message, .. } => (code, message),
                client::http::HttpError::Network(m) => ("NETWORK_ERROR".to_owned(), m),
                client::http::HttpError::InProgress => ("COMMAND_IN_PROGRESS".to_owned(), String::new()),
            });
            self.viewers.loaded(&path, result);
        }
        self.run_viewer_effects();
        changed || self.viewers.playing()
    }
}

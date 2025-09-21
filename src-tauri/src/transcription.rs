use tokio::sync::mpsc;

use crate::soniox::SonioxControl;

// Public, shared audio chunk type for all providers. For now, alias Soniox's.
pub type AudioChunk = crate::soniox::AudioChunk;

// Handle returned by providers; exposes a sending channel for audio.
#[derive(Clone)]
pub struct TranscriptionHandle {
    pub tx: mpsc::Sender<AudioChunk>,
    pub ctrl: Option<mpsc::Sender<SonioxControl>>,
}

// Enum of known providers for visibility in state/logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Soniox,
}

pub mod providers {
    use super::TranscriptionHandle;

    pub mod soniox_adapter {
        use super::TranscriptionHandle;
        use crate::soniox::{self, SonioxOptions};
        use tauri::AppHandle;

        pub async fn start_session(
            app: AppHandle,
            opts: SonioxOptions,
        ) -> Result<TranscriptionHandle, String> {
            let handle = soniox::start_session(app, opts).await?;
            Ok(TranscriptionHandle {
                tx: handle.tx,
                ctrl: Some(handle.ctrl),
            })
        }
    }
}

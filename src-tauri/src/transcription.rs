use tokio::sync::mpsc;

// Public, shared audio chunk type for all providers. For now, alias Soniox's.
pub type AudioChunk = crate::soniox::AudioChunk;

// Handle returned by providers; exposes a sending channel for audio.
#[derive(Clone)]
pub struct TranscriptionHandle {
  pub tx: mpsc::Sender<AudioChunk>,
}

// Enum of known providers for visibility in state/logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
  Soniox,
}

// Trait for real-time transcription providers.
// Start should spawn any background tasks and return a handle to push audio.
#[allow(unused_variables)]
pub trait TranscriptionProvider {
  fn kind(&self) -> ProviderKind;
}

pub mod providers {
  use super::{AudioChunk, TranscriptionHandle};

  pub mod soniox_adapter {
    use tauri::AppHandle;
    use crate::soniox::{self, SonioxOptions};
    use super::{AudioChunk, TranscriptionHandle};

    pub async fn start_session(app: AppHandle, opts: SonioxOptions) -> Result<TranscriptionHandle, String> {
      let handle = soniox::start_session(app, opts).await?;
      Ok(TranscriptionHandle { tx: handle.tx })
    }
  }
}


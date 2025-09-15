use std::fs::File;
use std::io::{BufWriter, Write};

use crate::lame_encoder;
use crate::utils::log_to_file;

pub enum AudioWriter {
  Wav(hound::WavWriter<BufWriter<File>>),
  Mp3 {
    encoder: lame_encoder::Lame,
    file: File,
    buffer: Vec<i16>,
    channels: u16,
  },
}

impl AudioWriter {
  pub fn write_sample(&mut self, sample: i16) -> Result<(), Box<dyn std::error::Error>> {
    match self {
      AudioWriter::Wav(writer) => {
        writer.write_sample(sample)?;
      }
      AudioWriter::Mp3 { buffer, channels, .. } => {
        buffer.push(sample);

        let samples_per_frame = 1152 * (*channels as usize);
        if buffer.len() >= samples_per_frame {
          self.flush_mp3_buffer()?;
        }
      }
    }
    Ok(())
  }

  fn flush_mp3_buffer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    if let AudioWriter::Mp3 { encoder, file, buffer, channels } = self {
      if buffer.is_empty() {
        return Ok(());
      }

      let samples_per_channel = buffer.len() / (*channels as usize);
      let mut left_channel = Vec::with_capacity(samples_per_channel);
      let mut right_channel = Vec::with_capacity(samples_per_channel);

      if *channels == 1 {
        left_channel = buffer.clone();
        right_channel = buffer.clone();
      } else {
        for chunk in buffer.chunks(2) {
          if chunk.len() >= 2 {
            left_channel.push(chunk[0]);
            right_channel.push(chunk[1]);
          } else if chunk.len() == 1 {
            left_channel.push(chunk[0]);
            right_channel.push(0);
          }
        }
      }

      let mut mp3_buffer = vec![0u8; (left_channel.len() * 5 / 4 + 7200).max(16384)];
      match encoder.encode(&left_channel, &right_channel, &mut mp3_buffer) {
        Ok(bytes_written) => {
          if bytes_written > 0 {
            file.write_all(&mp3_buffer[..bytes_written])?;
            log_to_file(&format!("MP3: Encoded {} samples -> {} bytes", buffer.len(), bytes_written));
          }
        }
        Err(e) => {
          log_to_file(&format!("MP3 encode error: {:?}", e));
        }
      }

      buffer.clear();
    }
    Ok(())
  }

  pub fn finalize(mut self) -> Result<(), Box<dyn std::error::Error>> {
    self.flush_mp3_buffer()?;

    match self {
      AudioWriter::Wav(writer) => {
        writer.finalize()?;
      }
      AudioWriter::Mp3 { mut encoder, mut file, .. } => {
        let mut final_buffer = vec![0u8; 7200];
        match encoder.flush(&mut final_buffer) {
          Ok(bytes_written) => {
            if bytes_written > 0 {
              file.write_all(&final_buffer[..bytes_written])?;
            }
          }
          Err(e) => {
            eprintln!("LAME flush error: {:?}", e);
          }
        }
        file.flush()?;
      }
    }
    Ok(())
  }
}


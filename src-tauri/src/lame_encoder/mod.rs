mod lame_ffi;

use lame_ffi::LamePtr;
use std::convert::From;
use std::ops::Drop;
use std::os::raw::c_int;
use std::ptr;

#[derive(Debug)]
pub enum Error {
    Ok,
    GenericError,
    NoMem,
    BadBitRate,
    BadSampleFreq,
    InternalError,
    Unknown(c_int),
}

impl From<c_int> for Error {
    fn from(errcode: c_int) -> Error {
        match errcode {
            0 => Error::Ok,
            -1 => Error::GenericError,
            -10 => Error::NoMem,
            -11 => Error::BadBitRate,
            -12 => Error::BadSampleFreq,
            -13 => Error::InternalError,
            _ => Error::Unknown(errcode),
        }
    }
}

fn handle_simple_error(retn: c_int) -> Result<(), Error> {
    match retn.into() {
        Error::Ok => Ok(()),
        err => Err(err),
    }
}

fn int_size(sz: usize) -> c_int {
    if sz > c_int::max_value() as usize {
        panic!("converting {} to c_int would overflow", sz);
    }

    sz as c_int
}

#[derive(Debug)]
pub enum EncodeError {
    OutputBufferTooSmall,
    NoMem,
    InitParamsNotCalled,
    PsychoAcousticError,
    Unknown(c_int),
}

/// Represents a Lame encoder context.
pub struct Lame {
    ptr: LamePtr,
}

// SAFETY: LAME encoder is thread-safe for independent operations
// Each Lame instance manages its own state via the opaque pointer
unsafe impl Send for Lame {}
unsafe impl Sync for Lame {}

impl Lame {
    /// Creates a new Lame encoder context with default parameters.
    ///
    /// Returns None if liblame could not allocate its internal structures.
    pub fn new() -> Option<Lame> {
        let ctx = unsafe { lame_ffi::lame_init() };

        if ctx == ptr::null_mut() {
            None
        } else {
            Some(Lame { ptr: ctx })
        }
    }

    /// Sample rate of input PCM data. Defaults to 44100 Hz.
    pub fn sample_rate(&self) -> u32 {
        unsafe { lame_ffi::lame_get_in_samplerate(self.ptr) as u32 }
    }

    /// Sets sample rate of input PCM data.
    pub fn set_sample_rate(&mut self, sample_rate: u32) -> Result<(), Error> {
        handle_simple_error(unsafe {
            lame_ffi::lame_set_in_samplerate(self.ptr, sample_rate as c_int)
        })
    }

    /// Number of channels in input stream. Defaults to 2.
    pub fn channels(&self) -> u8 {
        unsafe { lame_ffi::lame_get_num_channels(self.ptr) as u8 }
    }

    /// Sets number of channels in input stream.
    pub fn set_channels(&mut self, channels: u8) -> Result<(), Error> {
        handle_simple_error(unsafe { lame_ffi::lame_set_num_channels(self.ptr, channels as c_int) })
    }

    /// LAME quality parameter. See `set_quality` for more details.
    pub fn quality(&self) -> u8 {
        unsafe { lame_ffi::lame_get_quality(self.ptr) as u8 }
    }

    /// Sets LAME's quality parameter. True quality is determined by the
    /// bitrate but this parameter affects quality by influencing whether LAME
    /// selects expensive or cheap algorithms.
    ///
    /// This is a number from 0 to 9 (inclusive), where 0 is the best and
    /// slowest and 9 is the worst and fastest.
    pub fn set_quality(&mut self, quality: u8) -> Result<(), Error> {
        handle_simple_error(unsafe { lame_ffi::lame_set_quality(self.ptr, quality as c_int) })
    }

    /// Returns the output bitrate in kilobits per second.
    pub fn kilobitrate(&self) -> i32 {
        unsafe { lame_ffi::lame_get_brate(self.ptr) as i32 }
    }

    /// Sets the target output bitrate. This value is in kilobits per second,
    /// so passing 320 would select an output bitrate of 320kbps.
    pub fn set_kilobitrate(&mut self, quality: i32) -> Result<(), Error> {
        handle_simple_error(unsafe { lame_ffi::lame_set_brate(self.ptr, quality as c_int) })
    }

    /// Sets more internal parameters according to the other basic parameter
    /// settings.
    pub fn init_params(&mut self) -> Result<(), Error> {
        handle_simple_error(unsafe { lame_ffi::lame_init_params(self.ptr) })
    }

    /// Encodes PCM data into MP3 frames. The `pcm_left` and `pcm_right`
    /// buffers must be of the same length, or this function will panic.
    pub fn encode(
        &mut self,
        pcm_left: &[i16],
        pcm_right: &[i16],
        mp3_buffer: &mut [u8],
    ) -> Result<usize, EncodeError> {
        if pcm_left.len() != pcm_right.len() {
            panic!("left and right channels must have same number of samples!");
        }

        let retn = unsafe {
            lame_ffi::lame_encode_buffer(
                self.ptr,
                pcm_left.as_ptr(),
                pcm_right.as_ptr(),
                int_size(pcm_left.len()),
                mp3_buffer.as_mut_ptr(),
                int_size(mp3_buffer.len()),
            )
        };

        match retn {
            -1 => Err(EncodeError::OutputBufferTooSmall),
            -2 => Err(EncodeError::NoMem),
            -3 => Err(EncodeError::InitParamsNotCalled),
            -4 => Err(EncodeError::PsychoAcousticError),
            _ => {
                if retn < 0 {
                    Err(EncodeError::Unknown(retn))
                } else {
                    Ok(retn as usize)
                }
            }
        }
    }

    /// Flushes any remaining PCM data and returns the final MP3 frames.
    /// This should be called when done encoding to ensure all data is written.
    pub fn flush(&mut self, mp3_buffer: &mut [u8]) -> Result<usize, EncodeError> {
        let retn = unsafe {
            lame_ffi::lame_encode_flush(
                self.ptr,
                mp3_buffer.as_mut_ptr(),
                int_size(mp3_buffer.len()),
            )
        };

        match retn {
            -1 => Err(EncodeError::OutputBufferTooSmall),
            -2 => Err(EncodeError::NoMem),
            -3 => Err(EncodeError::InitParamsNotCalled),
            -4 => Err(EncodeError::PsychoAcousticError),
            _ => {
                if retn < 0 {
                    Err(EncodeError::Unknown(retn))
                } else {
                    Ok(retn as usize)
                }
            }
        }
    }
}

impl Drop for Lame {
    fn drop(&mut self) {
        unsafe { lame_ffi::lame_close(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_lame_encoder_creation() {
        let encoder = Lame::new();
        assert!(encoder.is_some(), "Should be able to create LAME encoder");
    }

    #[test]
    fn test_lame_encoder_configuration() {
        let mut encoder = Lame::new().expect("Failed to create LAME encoder");

        // Test setting sample rate
        assert!(encoder.set_sample_rate(44100).is_ok());
        assert_eq!(encoder.sample_rate(), 44100);

        // Test setting channels
        assert!(encoder.set_channels(2).is_ok());
        assert_eq!(encoder.channels(), 2);

        // Test setting quality
        assert!(encoder.set_quality(5).is_ok());
        assert_eq!(encoder.quality(), 5);

        // Test setting bitrate
        assert!(encoder.set_kilobitrate(128).is_ok());
        assert_eq!(encoder.kilobitrate(), 128);

        // Test initialization
        assert!(encoder.init_params().is_ok());
    }

    #[test]
    fn test_mp3_encoding_basic() {
        let mut encoder = Lame::new().expect("Failed to create LAME encoder");

        // Configure encoder
        assert!(encoder.set_sample_rate(44100).is_ok());
        assert!(encoder.set_channels(2).is_ok());
        assert!(encoder.set_kilobitrate(128).is_ok());
        assert!(encoder.set_quality(5).is_ok());
        assert!(encoder.init_params().is_ok());

        // Generate test audio data (1152 samples per channel, one MP3 frame)
        let samples_per_channel = 1152;
        let mut left_channel = vec![0i16; samples_per_channel];
        let mut right_channel = vec![0i16; samples_per_channel];

        // Generate a simple sine wave at 440Hz
        for i in 0..samples_per_channel {
            let t = i as f32 / 44100.0;
            let sample_value = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            let sample_i16 = (sample_value * 16384.0) as i16;
            left_channel[i] = sample_i16;
            right_channel[i] = sample_i16;
        }

        // Encode the audio
        let mut mp3_buffer = vec![0u8; 16384];
        let result = encoder.encode(&left_channel, &right_channel, &mut mp3_buffer);

        assert!(result.is_ok(), "Encoding should succeed");
        let bytes_written = result.unwrap();
        assert!(bytes_written > 0, "Should produce some MP3 data");
        println!(
            "Encoded {} samples into {} bytes",
            samples_per_channel * 2,
            bytes_written
        );
    }

    #[test]
    fn test_mp3_encoding_with_flush() {
        let mut encoder = Lame::new().expect("Failed to create LAME encoder");

        // Configure encoder
        assert!(encoder.set_sample_rate(44100).is_ok());
        assert!(encoder.set_channels(2).is_ok());
        assert!(encoder.set_kilobitrate(128).is_ok());
        assert!(encoder.set_quality(5).is_ok());
        assert!(encoder.init_params().is_ok());

        let mut total_bytes = 0;

        // Encode multiple frames
        for frame in 0..10 {
            let samples_per_channel = 1152;
            let mut left_channel = vec![0i16; samples_per_channel];
            let mut right_channel = vec![0i16; samples_per_channel];

            // Generate different frequency for each frame
            let frequency = 440.0 + (frame as f32 * 110.0);
            for i in 0..samples_per_channel {
                let t = i as f32 / 44100.0;
                let sample_value = (2.0 * std::f32::consts::PI * frequency * t).sin();
                let sample_i16 = (sample_value * 16384.0) as i16;
                left_channel[i] = sample_i16;
                right_channel[i] = sample_i16;
            }

            let mut mp3_buffer = vec![0u8; 16384];
            let result = encoder.encode(&left_channel, &right_channel, &mut mp3_buffer);
            assert!(result.is_ok(), "Frame {} encoding should succeed", frame);
            total_bytes += result.unwrap();
        }

        // Test flush
        let mut flush_buffer = vec![0u8; 7200];
        let flush_result = encoder.flush(&mut flush_buffer);
        assert!(flush_result.is_ok(), "Flush should succeed");
        total_bytes += flush_result.unwrap();

        assert!(total_bytes > 0, "Should have produced MP3 data");
        println!("Total MP3 bytes produced: {}", total_bytes);
    }

    #[test]
    fn test_mp3_file_creation() {
        let mut encoder = Lame::new().expect("Failed to create LAME encoder");

        // Configure encoder
        assert!(encoder.set_sample_rate(44100).is_ok());
        assert!(encoder.set_channels(2).is_ok());
        assert!(encoder.set_kilobitrate(128).is_ok());
        assert!(encoder.set_quality(5).is_ok());
        assert!(encoder.init_params().is_ok());

        // Create a temporary file
        let temp_path = std::env::temp_dir().join("test_encoding.mp3");
        let mut file = File::create(&temp_path).expect("Should create temp file");

        let mut total_bytes_written = 0;

        // Generate 1 second of audio (44100 samples per channel)
        let total_samples = 44100;
        let chunk_size = 1152; // LAME frame size

        for chunk_start in (0..total_samples).step_by(chunk_size) {
            let chunk_end = (chunk_start + chunk_size).min(total_samples);
            let chunk_len = chunk_end - chunk_start;

            let mut left_channel = vec![0i16; chunk_len];
            let mut right_channel = vec![0i16; chunk_len];

            // Generate sine wave
            for i in 0..chunk_len {
                let sample_idx = chunk_start + i;
                let t = sample_idx as f32 / 44100.0;
                let sample_value = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
                let sample_i16 = (sample_value * 16384.0) as i16;
                left_channel[i] = sample_i16;
                right_channel[i] = sample_i16;
            }

            let mut mp3_buffer = vec![0u8; 16384];
            let result = encoder.encode(&left_channel, &right_channel, &mut mp3_buffer);
            assert!(result.is_ok(), "Chunk encoding should succeed");

            let bytes_written = result.unwrap();
            if bytes_written > 0 {
                file.write_all(&mp3_buffer[..bytes_written])
                    .expect("Should write MP3 data to file");
                total_bytes_written += bytes_written;
            }
        }

        // Flush remaining data
        let mut flush_buffer = vec![0u8; 7200];
        let flush_result = encoder.flush(&mut flush_buffer);
        assert!(flush_result.is_ok(), "Flush should succeed");

        let flush_bytes = flush_result.unwrap();
        if flush_bytes > 0 {
            file.write_all(&flush_buffer[..flush_bytes])
                .expect("Should write flush data to file");
            total_bytes_written += flush_bytes;
        }

        file.flush().expect("Should flush file");
        drop(file);

        // Verify file was created and has content
        let metadata = std::fs::metadata(&temp_path).expect("File should exist");
        assert!(metadata.len() > 0, "MP3 file should have content");
        assert_eq!(
            metadata.len() as usize,
            total_bytes_written,
            "File size should match bytes written"
        );

        println!(
            "Created test MP3 file: {} ({} bytes)",
            temp_path.display(),
            total_bytes_written
        );

        // Cleanup
        std::fs::remove_file(&temp_path).expect("Should cleanup temp file");
    }

    #[test]
    fn test_different_quality_settings() {
        let quality_settings = [(64, 9), (128, 7), (192, 5), (320, 2)];

        for (bitrate, quality) in quality_settings.iter() {
            let mut encoder = Lame::new().expect("Failed to create LAME encoder");

            assert!(encoder.set_sample_rate(44100).is_ok());
            assert!(encoder.set_channels(2).is_ok());
            assert!(encoder.set_kilobitrate(*bitrate).is_ok());
            assert!(encoder.set_quality(*quality).is_ok());
            assert!(encoder.init_params().is_ok());

            // Verify settings were applied
            assert_eq!(encoder.kilobitrate(), *bitrate);
            assert_eq!(encoder.quality(), *quality);

            // Test encoding works with these settings
            let samples_per_channel = 1152;
            let left_channel = vec![100i16; samples_per_channel];
            let right_channel = vec![-100i16; samples_per_channel];

            let mut mp3_buffer = vec![0u8; 16384];
            let result = encoder.encode(&left_channel, &right_channel, &mut mp3_buffer);

            assert!(
                result.is_ok(),
                "Encoding with bitrate {} should succeed",
                bitrate
            );
            println!("Bitrate {}: {} bytes encoded", bitrate, result.unwrap());
        }
    }
}

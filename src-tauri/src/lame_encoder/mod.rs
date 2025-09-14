mod lame_ffi;

use std::ptr;
use std::ops::Drop;
use lame_ffi::LamePtr;
use std::os::raw::c_int;
use std::convert::From;

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
            lame_ffi::lame_set_in_samplerate(self.ptr, sample_rate as c_int) })
    }

    /// Number of channels in input stream. Defaults to 2.
    pub fn channels(&self) -> u8 {
        unsafe { lame_ffi::lame_get_num_channels(self.ptr) as u8 }
    }

    /// Sets number of channels in input stream.
    pub fn set_channels(&mut self, channels: u8) -> Result<(), Error> {
        handle_simple_error(unsafe {
            lame_ffi::lame_set_num_channels(self.ptr, channels as c_int) })
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
        handle_simple_error(unsafe {
            lame_ffi::lame_set_quality(self.ptr, quality as c_int) })
    }

    /// Returns the output bitrate in kilobits per second.
    pub fn kilobitrate(&self) -> i32 {
        unsafe { lame_ffi::lame_get_brate(self.ptr) as i32 }
    }

    /// Sets the target output bitrate. This value is in kilobits per second,
    /// so passing 320 would select an output bitrate of 320kbps.
    pub fn set_kilobitrate(&mut self, quality: i32) -> Result<(), Error> {
        handle_simple_error(unsafe {
            lame_ffi::lame_set_brate(self.ptr, quality as c_int) })
    }

    /// Sets more internal parameters according to the other basic parameter
    /// settings.
    pub fn init_params(&mut self) -> Result<(), Error> {
        handle_simple_error(unsafe {
            lame_ffi::lame_init_params(self.ptr) })
    }

    /// Encodes PCM data into MP3 frames. The `pcm_left` and `pcm_right`
    /// buffers must be of the same length, or this function will panic.
    pub fn encode(&mut self, pcm_left: &[i16], pcm_right: &[i16], mp3_buffer: &mut [u8]) -> Result<usize, EncodeError> {
        if pcm_left.len() != pcm_right.len() {
            panic!("left and right channels must have same number of samples!");
        }

        let retn = unsafe {
            lame_ffi::lame_encode_buffer(self.ptr,
                pcm_left.as_ptr(), pcm_right.as_ptr(), int_size(pcm_left.len()),
                mp3_buffer.as_mut_ptr(), int_size(mp3_buffer.len()))
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
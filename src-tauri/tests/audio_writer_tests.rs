use assistant_lib::{lame_encoder, AudioWriter};
use std::fs::File;
use tempfile::NamedTempFile;

#[test]
fn test_audio_writer_mp3_creation() {
    // Create a temporary file
    let temp_file = NamedTempFile::new().expect("Should create temp file");
    let temp_path = temp_file.path().to_path_buf();
    drop(temp_file); // Close the file so we can create our own

    // Create MP3 encoder
    let mut encoder = lame_encoder::Lame::new().expect("Failed to create LAME encoder");
    encoder.set_sample_rate(44100).expect("Set sample rate");
    encoder.set_channels(2).expect("Set channels");
    encoder.set_kilobitrate(128).expect("Set bitrate");
    encoder.set_quality(5).expect("Set quality");
    encoder.init_params().expect("Init params");

    let file = File::create(&temp_path).expect("Create file");

    let mut audio_writer = AudioWriter::Mp3 {
        encoder,
        file,
        buffer: Vec::new(),
        channels: 2,
    };

    // Write some samples (enough to trigger encoding)
    let samples_to_write = 1152 * 2 * 2; // 2 frames worth of stereo samples
    for i in 0..samples_to_write {
        let sample = (i as f32 / 100.0).sin() * 16384.0;
        audio_writer
            .write_sample(sample as i16)
            .expect("Write sample");
    }

    // Finalize
    audio_writer.finalize().expect("Finalize");

    // Check file was created and has content
    let metadata = std::fs::metadata(&temp_path).expect("File should exist");
    assert!(metadata.len() > 0, "MP3 file should have content");

    println!("Created MP3 file with {} bytes", metadata.len());

    // Cleanup
    std::fs::remove_file(&temp_path).expect("Cleanup");
}

#[test]
fn test_audio_writer_wav_creation() {
    use hound;

    // Create a temporary file
    let temp_file = NamedTempFile::new().expect("Should create temp file");
    let temp_path = temp_file.path().to_path_buf();
    drop(temp_file);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let wav_writer = hound::WavWriter::create(&temp_path, spec).expect("Create WAV writer");

    let mut audio_writer = AudioWriter::Wav(wav_writer);

    // Write some samples
    for i in 0..44100 {
        let sample = (i as f32 / 100.0).sin() * 16384.0;
        audio_writer
            .write_sample(sample as i16)
            .expect("Write sample");
    }

    // Finalize
    audio_writer.finalize().expect("Finalize");

    // Check file was created and has content
    let metadata = std::fs::metadata(&temp_path).expect("File should exist");
    assert!(metadata.len() > 0, "WAV file should have content");

    println!("Created WAV file with {} bytes", metadata.len());

    // Cleanup
    std::fs::remove_file(&temp_path).expect("Cleanup");
}

#[test]
fn test_mp3_quality_settings() {
    let quality_settings = [
        ("verylow", 64, 9),
        ("low", 128, 7),
        ("medium", 192, 5),
        ("high", 320, 2),
    ];

    for (quality_name, expected_bitrate, expected_quality) in quality_settings.iter() {
        let temp_file = NamedTempFile::new().expect("Should create temp file");
        let temp_path = temp_file.path().to_path_buf();
        drop(temp_file);

        let mut encoder = lame_encoder::Lame::new().expect("Failed to create LAME encoder");
        encoder.set_sample_rate(44100).expect("Set sample rate");
        encoder.set_channels(2).expect("Set channels");
        encoder
            .set_kilobitrate(*expected_bitrate)
            .expect("Set bitrate");
        encoder.set_quality(*expected_quality).expect("Set quality");
        encoder.init_params().expect("Init params");

        // Verify settings
        assert_eq!(encoder.kilobitrate(), *expected_bitrate);
        assert_eq!(encoder.quality(), *expected_quality);

        let file = File::create(&temp_path).expect("Create file");

        let mut audio_writer = AudioWriter::Mp3 {
            encoder,
            file,
            buffer: Vec::new(),
            channels: 2,
        };

        // Write enough samples to trigger encoding
        let samples_to_write = 1152 * 2 * 3; // 3 frames worth
        for i in 0..samples_to_write {
            let sample = (i as f32 / 100.0).sin() * 16384.0;
            audio_writer
                .write_sample(sample as i16)
                .expect("Write sample");
        }

        audio_writer.finalize().expect("Finalize");

        let metadata = std::fs::metadata(&temp_path).expect("File should exist");
        assert!(
            metadata.len() > 0,
            "MP3 file should have content for {}",
            quality_name
        );

        println!("{}: {} bytes", quality_name, metadata.len());

        std::fs::remove_file(&temp_path).expect("Cleanup");
    }
}

#[test]
fn test_mono_to_stereo_conversion() {
    let temp_file = NamedTempFile::new().expect("Should create temp file");
    let temp_path = temp_file.path().to_path_buf();
    drop(temp_file);

    let mut encoder = lame_encoder::Lame::new().expect("Failed to create LAME encoder");
    encoder.set_sample_rate(44100).expect("Set sample rate");
    encoder
        .set_channels(2)
        .expect("Set channels - stereo output");
    encoder.set_kilobitrate(128).expect("Set bitrate");
    encoder.set_quality(5).expect("Set quality");
    encoder.init_params().expect("Init params");

    let file = File::create(&temp_path).expect("Create file");

    let mut audio_writer = AudioWriter::Mp3 {
        encoder,
        file,
        buffer: Vec::new(),
        channels: 1, // Mono input
    };

    // Write mono samples (should be duplicated to stereo)
    let samples_to_write = 1152 * 3; // 3 frames worth of mono
    for i in 0..samples_to_write {
        let sample = (i as f32 / 100.0).sin() * 16384.0;
        audio_writer
            .write_sample(sample as i16)
            .expect("Write sample");
    }

    audio_writer.finalize().expect("Finalize");

    let metadata = std::fs::metadata(&temp_path).expect("File should exist");
    assert!(metadata.len() > 0, "MP3 file should have content");

    println!("Mono->Stereo MP3: {} bytes", metadata.len());

    std::fs::remove_file(&temp_path).expect("Cleanup");
}

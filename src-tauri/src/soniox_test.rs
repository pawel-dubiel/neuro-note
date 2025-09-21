#[cfg(test)]
mod soniox_tests {
    use crate::soniox::{AudioChunk, SonioxOptions};

    fn load_test_config() -> Result<SonioxOptions, String> {
        let mut candidates = Vec::new();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        candidates.push(cwd.join("config/soniox.local.json"));
        candidates.push(cwd.join("../config/soniox.local.json"));

        for path in candidates {
            if let Ok(txt) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                    let api_key = json
                        .get("api_key")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing api_key in config")?
                        .to_string();

                    let audio_format = json
                        .get("audio_format")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pcm_s16le")
                        .to_string();

                    let translation = json
                        .get("translation")
                        .and_then(|v| v.as_str())
                        .unwrap_or("none")
                        .to_string();

                    return Ok(SonioxOptions {
                        api_key,
                        audio_format,
                        translation,
                    });
                }
            }
        }

        Err("Could not load config/soniox.local.json".to_string())
    }

    #[test]
    fn test_soniox_config_loading() {
        let config = load_test_config();
        assert!(config.is_ok(), "Should be able to load Soniox config");

        let opts = config.unwrap();
        assert!(!opts.api_key.is_empty(), "API key should not be empty");
        assert_eq!(opts.audio_format, "pcm_s16le");
        assert_eq!(opts.translation, "none");
    }

    #[test]
    fn test_audio_conversion() {
        // Test mono 16kHz (should be identity)
        let samples: Vec<i16> = (0..160).map(|i| i as i16).collect();
        let result = crate::soniox::to_pcm16_mono_16k(&samples, 1, 16000);
        assert_eq!(result.len(), samples.len() * 2); // 2 bytes per i16

        // Test stereo downmix
        let stereo_samples: Vec<i16> = vec![1000, 3000, 1000, 3000]; // L=1000, R=3000
        let result = crate::soniox::to_pcm16_mono_16k(&stereo_samples, 2, 16000);
        assert_eq!(result.len(), 4); // 2 samples * 2 bytes each
        let first_sample = i16::from_le_bytes([result[0], result[1]]);
        assert_eq!(first_sample, 2000); // Average of 1000 and 3000

        // Test resampling from 8kHz to 16kHz
        let samples_8k: Vec<i16> = (0..80).map(|i| (i * 100) as i16).collect();
        let result = crate::soniox::to_pcm16_mono_16k(&samples_8k, 1, 8000);
        assert!(result.len() >= samples_8k.len() * 2); // Should be roughly doubled
    }

    #[test]
    fn test_token_rendering() {
        use serde_json::json;

        let final_tokens = vec![
            json!({"text": "Hello ", "is_final": true, "speaker": "1", "language": "en"}),
            json!({"text": "world", "is_final": true, "speaker": "1", "language": "en"}),
        ];

        let non_final =
            vec![json!({"text": "!", "is_final": false, "speaker": "1", "language": "en"})];

        let result = crate::soniox::render_tokens(&final_tokens, &non_final);

        assert!(result.contains("Speaker 1:"));
        assert!(result.contains("[en]"));
        assert!(result.contains("Hello world!"));
        assert!(result.contains("==============================="));
    }

    #[test]
    fn test_multi_speaker_rendering() {
        use serde_json::json;

        let tokens = vec![
            json!({"text": "Hello", "is_final": true, "speaker": "1", "language": "en"}),
            json!({"text": "Hi there", "is_final": true, "speaker": "2", "language": "en"}),
        ];

        let result = crate::soniox::render_tokens(&tokens, &vec![]);

        assert!(result.contains("Speaker 1:"));
        assert!(result.contains("Speaker 2:"));
        assert!(result.contains("Hello"));
        assert!(result.contains("Hi there"));
    }

    #[test]
    fn test_audio_chunk_creation() {
        let chunk = AudioChunk {
            samples: vec![100, 200, 300],
            channels: 1,
            sample_rate: 16000,
        };

        assert_eq!(chunk.samples.len(), 3);
        assert_eq!(chunk.channels, 1);
        assert_eq!(chunk.sample_rate, 16000);
    }
}

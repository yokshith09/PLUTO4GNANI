//! Helpers for making the synthetic test recordings (T-008): read a script, convert speech to the
//! 16 kHz mono WAV the app takes. Used by `examples/make_test_audio.rs`; no network here.

/// One spoken line of a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub speaker: String,
    pub text: String,
}

/// Lines of the form `**Name:** words` from a script file. The `(overlap)` stage direction is
/// dropped; anything else (headings, notes) is ignored.
pub fn parse_script(markdown: &str) -> Vec<Line> {
    markdown
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("**")?;
            let (speaker, text) = rest.split_once(":**")?;
            let text = text.replace("(overlap)", "");
            let text = text.trim();
            (!speaker.trim().is_empty() && !text.is_empty()).then(|| Line {
                speaker: speaker.trim().to_owned(),
                text: text.to_owned(),
            })
        })
        .collect()
}

/// PCM16 24 kHz bytes to samples (an odd trailing byte is dropped).
pub fn pcm_bytes_to_samples(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect()
}

/// 24 kHz to 16 kHz by linear interpolation (every 3 input samples become 2).
pub fn resample_24k_to_16k(input: &[i16]) -> Vec<i16> {
    let out_len = input.len() * 2 / 3;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * 1.5;
            let (a, frac) = (pos.floor() as usize, pos.fract());
            let x = f64::from(input[a]);
            let y = f64::from(input.get(a + 1).copied().unwrap_or(input[a]));
            (x + (y - x) * frac).round() as i16
        })
        .collect()
}

/// `ms` of silence at 16 kHz.
pub fn silence_16k(ms: u32) -> Vec<i16> {
    vec![0; (ms * 16) as usize]
}

/// A complete 16 kHz mono PCM16 WAV file.
pub fn wav_16k(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_scripts() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/audio-scripts");
        let as1 = parse_script(
            &std::fs::read_to_string(dir.join("as1-hinglish-launch-sync.md")).unwrap(),
        );
        assert_eq!(as1.len(), 21);
        let mut who: Vec<_> = as1.iter().map(|l| l.speaker.as_str()).collect();
        who.sort_unstable();
        who.dedup();
        assert_eq!(who, ["Asha", "Meena", "Ravi"]);
        assert!(as1.iter().all(|l| !l.text.contains("overlap")));
        assert!(as1[0].text.starts_with("Okay team"));
        assert!(parse_script("# heading\nplain text\n**: x\n**Name:** \n").is_empty());
    }

    #[test]
    fn resampling_keeps_two_thirds_of_the_samples_and_the_shape() {
        let ramp: Vec<i16> = (0..30).map(|i| i * 100).collect();
        let out = resample_24k_to_16k(&ramp);
        assert_eq!(out.len(), 20);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 150, "halfway between samples 1 and 2");
        assert!(out.windows(2).all(|w| w[0] < w[1]), "stays a ramp");
        assert!(resample_24k_to_16k(&[]).is_empty());
    }

    #[test]
    fn wav_header_and_sizes_are_right() {
        let w = wav_16k(&[1, -1, 2]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 6);
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), 16_000);
        assert_eq!(silence_16k(250).len(), 4000);
        assert_eq!(pcm_bytes_to_samples(&[1, 0, 255, 255, 9]), [1, -1]);
    }
}

//! Turns what the sound system hands over (any sample rate, any number of channels, 32-bit float
//! or 16-bit) into the recorder's format: 16 kHz, mono, 16-bit. No operating system in here, so it
//! is tested everywhere.
//!
//! Resampling averages the input over each output sample's time slot (a box filter), which is a
//! simple low-pass: speech survives and high-pitched content does not alias into it.

/// The recorder's sample rate.
pub const TARGET_RATE: u32 = 16_000;

/// Streaming converter: feed it blocks of interleaved samples as they arrive, get 16 kHz mono
/// samples back. Keeps the unfinished output sample between blocks, so block sizes do not matter.
#[derive(Debug, Clone)]
pub struct To16kMono {
    channels: usize,
    /// Input samples (per channel) that make one output sample.
    step: f64,
    /// Sum of the input so far in the current output sample, and how much of it is filled.
    acc: f64,
    filled: f64,
}

impl To16kMono {
    /// `rate` is the input sample rate; `channels` the number of interleaved channels (at least 1).
    pub fn new(rate: u32, channels: usize) -> Self {
        Self {
            channels: channels.max(1),
            step: f64::from(rate.max(1)) / f64::from(TARGET_RATE),
            acc: 0.0,
            filled: 0.0,
        }
    }

    /// Interleaved 32-bit float samples (-1.0 to 1.0).
    pub fn push_f32(&mut self, interleaved: &[f32]) -> Vec<i16> {
        let mut out = Vec::with_capacity(interleaved.len() / self.channels / 2 + 1);
        for frame in interleaved.chunks_exact(self.channels) {
            let mono = frame.iter().sum::<f32>() / self.channels as f32;
            self.feed(f64::from(mono), &mut out);
        }
        out
    }

    /// Interleaved 16-bit samples.
    pub fn push_i16(&mut self, interleaved: &[i16]) -> Vec<i16> {
        let mut out = Vec::with_capacity(interleaved.len() / self.channels / 2 + 1);
        for frame in interleaved.chunks_exact(self.channels) {
            let sum: f32 = frame.iter().map(|&s| f32::from(s)).sum();
            let mono = sum / self.channels as f32 / 32768.0;
            self.feed(f64::from(mono), &mut out);
        }
        out
    }

    /// One input sample of mono audio, covering one unit of input time.
    fn feed(&mut self, sample: f64, out: &mut Vec<i16>) {
        let mut left = 1.0; // how much of this input sample is still unspent
        loop {
            let room = self.step - self.filled;
            if left < room - 1e-9 {
                self.acc += sample * left;
                self.filled += left;
                return;
            }
            // This sample completes the output sample.
            self.acc += sample * room;
            out.push(to_i16(self.acc / self.step));
            left -= room;
            self.acc = 0.0;
            self.filled = 0.0;
            if left <= 1e-9 {
                return;
            }
        }
    }
}

fn to_i16(v: f64) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f64, seconds: f64, channels: usize) -> Vec<f32> {
        let n = (f64::from(rate) * seconds) as usize;
        (0..n)
            .flat_map(|i| {
                let v = (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin() as f32
                    * 0.5;
                std::iter::repeat_n(v, channels)
            })
            .collect()
    }

    fn peak(s: &[i16]) -> i16 {
        s.iter().map(|v| v.saturating_abs()).max().unwrap_or(0)
    }

    #[test]
    fn the_length_is_right_for_common_rates() {
        for (rate, channels) in [
            (48_000, 2),
            (44_100, 2),
            (16_000, 1),
            (8_000, 1),
            (96_000, 6),
        ] {
            let mut c = To16kMono::new(rate, channels);
            let out = c.push_f32(&sine(rate, 440.0, 1.0, channels));
            let want = 16_000_i64;
            assert!(
                (out.len() as i64 - want).abs() <= 1,
                "{rate} Hz x{channels}: {}",
                out.len()
            );
        }
    }

    #[test]
    fn speech_range_tones_keep_their_level_and_very_high_ones_are_filtered_out() {
        let mut c = To16kMono::new(48_000, 2);
        let tone = c.push_f32(&sine(48_000, 440.0, 0.5, 2));
        // 0.5 amplitude is about 16000; a box filter loses a little of a 440 Hz tone, not much.
        assert!(
            peak(&tone) > 15_500 && peak(&tone) < 16_500,
            "{}",
            peak(&tone)
        );
        // 20 kHz is far above what 16 kHz can carry: it must not come through as a loud alias.
        let mut c = To16kMono::new(48_000, 2);
        let high = c.push_f32(&sine(48_000, 20_000.0, 0.5, 2));
        assert!(peak(&high) < 4_000, "{}", peak(&high));
    }

    #[test]
    fn block_size_does_not_change_the_result() {
        let input = sine(44_100, 300.0, 0.7, 2);
        let whole = To16kMono::new(44_100, 2).push_f32(&input);
        let mut c = To16kMono::new(44_100, 2);
        let mut pieces = Vec::new();
        for block in input.chunks(2 * 97) {
            pieces.extend(c.push_f32(block));
        }
        assert_eq!(whole, pieces);
    }

    #[test]
    fn channels_are_averaged_and_silence_and_clipping_are_safe() {
        // Left loud, right silent: the mono mix is half.
        let mut c = To16kMono::new(16_000, 2);
        let out = c.push_f32(&[0.8, 0.0, 0.8, 0.0]);
        assert_eq!(out, vec![13_107, 13_107]);
        assert!(To16kMono::new(48_000, 2)
            .push_f32(&vec![0.0; 960])
            .iter()
            .all(|&s| s == 0));
        // Over-range input clips instead of wrapping.
        let loud = To16kMono::new(16_000, 1).push_f32(&[3.0, -3.0]);
        assert_eq!(loud, vec![32_767, -32_767]);
        // An empty block and a half frame produce nothing and do not panic.
        assert!(To16kMono::new(48_000, 2).push_f32(&[]).is_empty());
        assert!(To16kMono::new(48_000, 2).push_f32(&[0.1]).is_empty());
    }

    #[test]
    fn sixteen_bit_input_gives_the_same_as_float() {
        let f: Vec<f32> = sine(48_000, 500.0, 0.2, 2);
        let i: Vec<i16> = f.iter().map(|v| (v * 32767.0) as i16).collect();
        let a = To16kMono::new(48_000, 2).push_f32(&f);
        let b = To16kMono::new(48_000, 2).push_i16(&i);
        assert_eq!(a.len(), b.len());
        assert!(a
            .iter()
            .zip(&b)
            .all(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() < 8));
    }

    #[test]
    fn slower_input_is_stretched_up() {
        // 8 kHz mono in: twice as many samples out.
        let out = To16kMono::new(8_000, 1).push_f32(&[0.5; 800]);
        assert_eq!(out.len(), 1_600);
        assert!(out.iter().all(|&s| (s - 16_384).abs() <= 1));
    }
}

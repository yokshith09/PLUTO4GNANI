//! Ends a hands-free turn by itself once the person stops talking (the toggle hotkey): speech is
//! whatever is clearly above the room's background level, which is followed slowly. The same idea
//! as the always-on listener's and the Talk button's, here for frames that arrive in Rust.

use pluto_contracts::speech::FRAME_BYTES;

/// Frames are 32 ms.
const FRAME_MS: u64 = 32;

fn frames(ms: u64) -> u64 {
    ms.div_ceil(FRAME_MS)
}

#[derive(Debug, Clone, Copy)]
pub struct EndpointPolicy {
    /// A pause this long after speech ends the command.
    pub end_silence_ms: u64,
    /// No speech at all for this long: give up and send what there is.
    pub no_speech_ms: u64,
    /// The longest command.
    pub max_ms: u64,
}

impl Default for EndpointPolicy {
    fn default() -> Self {
        Self {
            end_silence_ms: 1_200,
            no_speech_ms: 6_000,
            max_ms: 30_000,
        }
    }
}

#[derive(Debug)]
pub struct Endpointer {
    policy: EndpointPolicy,
    floor: f32,
    heard_speech: bool,
    quiet: u64,
    count: u64,
}

impl Endpointer {
    pub fn new(policy: EndpointPolicy) -> Self {
        Self {
            policy,
            floor: 150.0,
            heard_speech: false,
            quiet: 0,
            count: 0,
        }
    }

    /// One captured frame (16 kHz PCM16, little endian). True once the command is over.
    pub fn feed(&mut self, pcm: &[u8]) -> bool {
        if pcm.len() != FRAME_BYTES {
            return false;
        }
        self.count += 1;
        let level = rms(pcm);
        if level >= (self.floor * 2.5).max(350.0) {
            self.heard_speech = true;
            self.quiet = 0;
        } else {
            self.floor = self.floor * 0.95 + level * 0.05;
            if self.heard_speech {
                self.quiet += 1;
            }
        }
        (self.heard_speech && self.quiet >= frames(self.policy.end_silence_ms))
            || (!self.heard_speech && self.count >= frames(self.policy.no_speech_ms))
            || self.count >= frames(self.policy.max_ms)
    }
}

fn rms(pcm: &[u8]) -> f32 {
    let n = pcm.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = pcm
        .chunks_exact(2)
        .map(|b| {
            let v = f64::from(i16::from_le_bytes([b[0], b[1]]));
            v * v
        })
        .sum();
    (sum / n as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(amp: i16) -> Vec<u8> {
        std::iter::repeat_n(amp.to_le_bytes(), FRAME_BYTES / 2)
            .flatten()
            .collect()
    }

    fn run(e: &mut Endpointer, amp: i16, ms: u64) -> bool {
        (0..frames(ms)).any(|_| e.feed(&frame(amp)))
    }

    #[test]
    fn a_pause_after_speech_ends_it_but_a_short_breath_does_not() {
        let mut e = Endpointer::new(EndpointPolicy::default());
        assert!(!run(&mut e, 5000, 1500));
        assert!(!run(&mut e, 20, 800));
        assert!(!run(&mut e, 5000, 600));
        assert!(run(&mut e, 20, 1500));
    }

    #[test]
    fn nothing_said_gives_up() {
        let mut e = Endpointer::new(EndpointPolicy::default());
        assert!(!run(&mut e, 20, 5000));
        assert!(run(&mut e, 20, 2000));
    }

    #[test]
    fn the_longest_command_is_cut_off_even_while_talking() {
        let mut e = Endpointer::new(EndpointPolicy {
            max_ms: 3_000,
            ..EndpointPolicy::default()
        });
        assert!(run(&mut e, 5000, 3500));
    }

    #[test]
    fn odd_sized_frames_are_ignored() {
        let mut e = Endpointer::new(EndpointPolicy::default());
        assert!(!e.feed(&[0; 10]));
    }
}

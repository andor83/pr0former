use pr0_dsp::AudioPlugin;

/// Example trusted source plugin. One interleaved block in, one block out.
pub struct Gain {
    pub target: f32,
    current: f32,
    coefficient: f32,
}
impl Default for Gain {
    fn default() -> Self {
        Self {
            target: 1.,
            current: 0.,
            coefficient: 0.004,
        }
    }
}
impl AudioPlugin for Gain {
    fn prepare(&mut self, sample_rate: f64, channels: usize) {
        self.coefficient = (1. - (-1. / (0.005 * sample_rate * channels as f64)).exp()) as f32;
    }
    fn reset(&mut self) {
        self.current = 0.;
    }
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            self.current += (self.target - self.current) * self.coefficient;
            *y = *x * self.current;
        }
    }
}

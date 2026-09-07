//! Same-width channel routing with 5 ms smoothing on live route changes.
#[derive(Default)]
pub struct ChannelMap {
    weights: [[f64; 8]; 8],
    initialized: bool,
}
impl ChannelMap {
    pub fn tick(
        &mut self,
        input: [f64; 8],
        routes: [usize; 8],
        channels: usize,
        sample_rate: f64,
    ) -> [f64; 8] {
        let alpha = 1. - (-1. / (0.005 * sample_rate)).exp();
        let mut output = [0.; 8];
        for destination in 0..channels {
            for source in 0..channels {
                let target = (routes[destination] == source + 1) as u8 as f64;
                let weight = &mut self.weights[destination][source];
                if !self.initialized {
                    *weight = target;
                } else {
                    *weight += (target - *weight) * alpha;
                }
                output[destination] += input[source] * *weight;
            }
        }
        self.initialized = true;
        output
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_permute_duplicate_and_mute_at_every_width() {
        let input = [1., 2., 3., 4., 5., 6., 7., 8.];
        for width in 1..=8 {
            let mut map = ChannelMap::default();
            let routes = std::array::from_fn(|i| if i < width { width - i } else { 0 });
            let result = map.tick(input, routes, width, 48000.);
            for i in 0..width {
                assert_eq!(result[i], input[width - i - 1]);
            }
            assert!(result[width..].iter().all(|v| *v == 0.));
            let mut duplicate = ChannelMap::default();
            assert!(
                duplicate.tick(input, [1; 8], width, 48000.)[..width]
                    .iter()
                    .all(|v| *v == 1.)
            );
            let mut mute = ChannelMap::default();
            assert_eq!(mute.tick(input, [0; 8], width, 48000.), [0.; 8]);
        }
        assert_eq!(
            ChannelMap::default().tick(input, [8; 8], 2, 48000.),
            [0.; 8]
        );
    }
    #[test]
    fn live_route_changes_crossfade_without_an_abrupt_jump() {
        let mut map = ChannelMap::default();
        let input = [1., -1., 0., 0., 0., 0., 0., 0.];
        assert_eq!(map.tick(input, [1; 8], 2, 48000.)[0], 1.);
        let first = map.tick(input, [2; 8], 2, 48000.)[0];
        assert!(first > 0.99 && first < 1.);
        for _ in 0..4800 {
            let result = map.tick(input, [2; 8], 2, 48000.);
            assert!(result.iter().all(|v| v.is_finite() && v.abs() <= 1.));
        }
        assert!((map.tick(input, [2; 8], 2, 48000.)[0] + 1.).abs() < 1e-7);
    }
}

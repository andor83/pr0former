use pr0_core::midi::Message;

pub struct Controllers {
    pub pending: [Option<Message>; 8],
    pub values: [f64; 8],
    pub channels: [u8; 8],
    pub numbers: [u8; 8],
    pub count: usize,
    pub learning: Option<usize>,
    pub learned: Option<(usize, u8, u8)>,
    pub serial: u64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub decimals: Option<i32>,
}
impl Controllers {
    pub fn value(&self, value: f64) -> f64 {
        let mut value = value.clamp(self.min, self.max);
        if self.step > 0. {
            value = self.min + ((value - self.min) / self.step).round() * self.step;
        }
        if let Some(decimals) = self.decimals {
            let scale = 10_f64.powi(decimals);
            return (value * scale)
                .round()
                .clamp((self.min * scale).ceil(), (self.max * scale).floor())
                / scale;
        }
        value.clamp(self.min, self.max)
    }
    pub fn set(&mut self, index: usize, value: f64) -> Message {
        self.values[index] = self.value(value);
        let unit = if self.max > self.min {
            (self.values[index] - self.min) / (self.max - self.min)
        } else {
            0.
        };
        Message {
            status: 0xb0 | self.channels[index],
            data1: self.numbers[index],
            data2: (unit * 127.).round() as u8,
        }
    }
    pub fn receive(&mut self, message: Message) {
        if message.status >> 4 != 11 {
            return;
        }
        if matches!(message.data1, 120 | 123) {
            return;
        }
        if let Some(index) = self.learning.take() {
            self.channels[index] = message.status & 15;
            self.numbers[index] = message.data1;
            self.learned = Some((index, message.status & 15, message.data1));
            self.serial = self.serial.wrapping_add(1);
        }
        for index in 0..self.count {
            if self.channels[index] == message.status & 15 && self.numbers[index] == message.data1 {
                self.values[index] =
                    self.value(self.min + f64::from(message.data2) / 127. * (self.max - self.min));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integer_outputs_stay_integer_at_fractional_range_boundaries() {
        let controls = Controllers {
            pending: [None; 8],
            values: [0.; 8],
            channels: [0; 8],
            numbers: [1; 8],
            count: 1,
            learning: None,
            learned: None,
            serial: 0,
            min: 0.2,
            max: 4.8,
            step: 0.,
            decimals: Some(0),
        };
        assert_eq!(controls.value(0.2), 1.);
        assert_eq!(controls.value(4.8), 4.);
        assert_eq!(controls.value(2.6), 3.);
    }
}

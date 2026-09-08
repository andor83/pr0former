#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    pub pitch: u8,
    pub velocity: u8,
}
#[derive(Default)]
pub struct NoteInputs {
    on: bool,
    off: bool,
    gate: bool,
}
impl NoteInputs {
    pub fn tick(&mut self, values: [f64; 5], connected: [bool; 5]) -> [Option<NoteEvent>; 2] {
        let [pitch, velocity, gate, on, off] = values;
        let pitch = pitch.round().clamp(0., 127.) as u8;
        let velocity = if connected[1] {
            velocity.round().clamp(0., 127.) as u8
        } else {
            100
        };
        let attack = if connected[3] {
            on > 0. && !self.on
        } else {
            connected[2] && gate > 0. && !self.gate
        };
        let release = if connected[4] {
            off > 0. && !self.off
        } else {
            connected[2] && gate <= 0. && self.gate
        };
        self.on = on > 0.;
        self.off = off > 0.;
        self.gate = gate > 0.;
        [
            release.then_some(NoteEvent { pitch, velocity: 0 }),
            attack.then_some(NoteEvent { pitch, velocity }),
        ]
    }
}

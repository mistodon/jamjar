use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LoopMode {
    Off,
    Record,
    Playback(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRecording<StartState: Clone, FrameInput: Clone + PartialEq> {
    pub start_state: StartState,
    pub frame_inputs: RleVec<FrameInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState<StartState: Clone, FrameInput: Clone + PartialEq> {
    mode: LoopMode,
    recording: Option<LoopRecording<StartState, FrameInput>>,
}

impl<S: Clone, F: Clone + PartialEq> LoopState<S, F> {
    pub fn new() -> Self {
        LoopState {
            mode: LoopMode::Off,
            recording: None,
        }
    }

    pub fn get_recording(&self) -> Option<&LoopRecording<S, F>> {
        self.recording.as_ref()
    }

    pub fn set_recording(&mut self, recording: LoopRecording<S, F>) {
        self.recording = Some(recording);
    }

    pub fn recorded_frames(&self) -> Option<usize> {
        self.recording.as_ref().map(|r| r.frame_inputs.len())
    }

    pub fn recording(&self) -> bool {
        match self.mode {
            LoopMode::Record => true,
            _ => false,
        }
    }

    pub fn playing(&self) -> bool {
        match self.mode {
            LoopMode::Playback(_) => true,
            _ => false,
        }
    }

    pub fn stop_recording(&mut self, state: &mut S) {
        if let (LoopMode::Record, Some(recording)) = (self.mode, &self.recording) {
            *state = recording.start_state.clone();
        }
        self.mode = LoopMode::Off;
    }

    pub fn stop_playback(&mut self, state: &mut S) {
        if let (LoopMode::Playback(_), Some(recording)) = (self.mode, &self.recording) {
            *state = recording.start_state.clone();
        }
        self.mode = LoopMode::Off;
    }

    pub fn start_recording(&mut self, state: &mut S) {
        if self.recording() || self.playing() {
            if let Some(recording) = &self.recording {
                *state = recording.start_state.clone();
            }
        }
        self.recording = Some(LoopRecording {
            start_state: state.clone(),
            frame_inputs: RleVec::with_capacity(8),
        });
        self.mode = LoopMode::Record;
    }

    pub fn start_playback(&mut self, state: &mut S) {
        if let Some(recording) = &self.recording {
            if !recording.frame_inputs.is_empty() {
                *state = recording.start_state.clone();
                self.mode = LoopMode::Playback(0);
            }
        }
    }

    pub fn toggle_recording(&mut self, state: &mut S) -> bool {
        if self.recording() {
            self.stop_recording(state);
        } else {
            self.start_recording(state);
        }
        self.recording()
    }

    pub fn toggle_playback(&mut self, state: &mut S) -> bool {
        if self.playing() {
            self.stop_playback(state);
        } else {
            self.start_playback(state);
        }
        self.playing()
    }

    pub fn frame_input(&mut self, state: &mut S, real_inputs: F) -> F {
        match self.mode {
            LoopMode::Off => real_inputs,
            LoopMode::Record => {
                let recording = self.recording.as_mut().unwrap();
                recording.frame_inputs.push(real_inputs.clone());
                real_inputs
            }
            LoopMode::Playback(frame) => {
                let recording = self.recording.as_ref().unwrap();
                let result = recording.frame_inputs[frame].clone();
                let next_frame = (frame + 1) % recording.frame_inputs.len();

                if next_frame == 0 {
                    *state = recording.start_state.clone();
                }

                self.mode = LoopMode::Playback(next_frame);
                result
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RleVec<V: PartialEq> {
    pub counts: Vec<usize>,
    pub values: Vec<V>,
}

impl<V: PartialEq> Default for RleVec<V> {
    fn default() -> Self {
        RleVec::new()
    }
}

impl<V: PartialEq> RleVec<V> {
    pub fn new() -> Self {
        RleVec {
            counts: Vec::new(),
            values: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        RleVec {
            counts: Vec::with_capacity(cap),
            values: Vec::with_capacity(cap),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.counts.iter().sum()
    }

    pub fn push(&mut self, value: V) {
        if Some(&value) == self.values.last() {
            *self.counts.last_mut().unwrap() += 1;
        } else {
            self.counts.push(1);
            self.values.push(value);
        }
    }
}

impl<V: PartialEq> std::ops::Index<usize> for RleVec<V> {
    type Output = V;

    fn index(&self, index: usize) -> &V {
        let mut total = 0;
        for (i, count) in self.counts.iter().copied().enumerate() {
            total += count;
            if index < total {
                return &self.values[i];
            }
        }
        panic!(
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
        );
    }
}

#[cfg(test)]
mod rlevec_tests {
    use super::*;

    #[test]
    fn size_tests() {
        let mut v: RleVec<char> = RleVec::new();
        assert_eq!(v.len(), 0);
        assert!(v.is_empty());

        v.push('a');
        assert_eq!(v.len(), 1);
        assert!(!v.is_empty());

        v.push('a');
        assert_eq!(v.len(), 2);

        v.push('b');
        assert_eq!(v.len(), 3);

        v.push('a');
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn content_tests() {
        let mut v: RleVec<char> = RleVec::new();

        v.push('a');
        assert_eq!(
            v,
            RleVec {
                counts: vec![1],
                values: vec!['a'],
            }
        );

        v.push('a');
        assert_eq!(
            v,
            RleVec {
                counts: vec![2],
                values: vec!['a'],
            }
        );

        v.push('b');
        assert_eq!(
            v,
            RleVec {
                counts: vec![2, 1],
                values: vec!['a', 'b'],
            }
        );

        v.push('a');
        assert_eq!(
            v,
            RleVec {
                counts: vec![2, 1, 1],
                values: vec!['a', 'b', 'a'],
            }
        );
    }

    #[test]
    fn index_tests() {
        let mut v: RleVec<char> = RleVec::new();
        v.push('a');
        v.push('a');
        v.push('b');
        v.push('a');
        v.push('c');

        assert_eq!(v[0], 'a');
        assert_eq!(v[1], 'a');
        assert_eq!(v[2], 'b');
        assert_eq!(v[3], 'a');
        assert_eq!(v[4], 'c');
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LoopMode {
    Off,
    Record,
    Playback(usize),
}

#[derive(Debug, Clone)]
pub struct LoopRecording<StartState: Clone, FrameInput: Clone> {
    pub start_state: StartState,
    pub frame_inputs: Vec<FrameInput>,
}

#[derive(Debug, Clone)]
pub struct LoopState<StartState: Clone, FrameInput: Clone> {
    mode: LoopMode,
    recording: Option<LoopRecording<StartState, FrameInput>>,
}

impl<S: Clone, F: Clone> LoopState<S, F> {
    pub fn new() -> Self {
        LoopState {
            mode: LoopMode::Off,
            recording: None,
        }
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
        if let Some(recording) = &self.recording {
            *state = recording.start_state.clone();
        }
        self.recording = Some(LoopRecording {
            start_state: state.clone(),
            frame_inputs: Vec::with_capacity(600),
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

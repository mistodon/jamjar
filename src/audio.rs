use std::{
    borrow::Cow,
    cmp::Eq,
    collections::HashMap,
    hash::Hash,
    io::Cursor,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, Sender};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

pub const MAX_TRACKS: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct Sound<K> {
    pub key: K,
    pub volume: f32,
    pub speed: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track<K: Clone> {
    pub key: K,
    pub settings: TrackSettings,
}

impl<K: Clone> Track<K> {
    fn id(&self) -> (K, Option<Duration>) {
        (self.key.clone(), self.settings.feedback_rate)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackSettings {
    pub volume: f32,
    pub playing: bool,
    pub looping: bool,
    pub feedback_rate: Option<Duration>,
}

impl Default for TrackSettings {
    fn default() -> Self {
        TrackSettings {
            volume: 1.,
            playing: true,
            looping: false,
            feedback_rate: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackSet<K: Clone> {
    pub track: Track<K>,
    pub hint: Option<Track<K>>,
}

impl<K: Clone> TrackSet<K> {
    fn hint_id(&self) -> Option<(K, Option<Duration>)> {
        self.hint.as_ref().map(|t| t.id())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feedback<K: Clone> {
    pub track: usize,
    pub key: K,
    pub data: FeedbackData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackData {
    Tick,
    Looped,
    Ended,
}

#[derive(Debug, Clone)]
enum AudioCmd<K: Clone> {
    Quit,
    Prewarm,
    LoadAudio(K, Cow<'static, [u8]>, Option<f32>),
    PlaySound(Sound<K>),
    SetMasterVolume {
        track: Option<f32>,
        sound: Option<f32>,
    },
    SetTrack {
        index: usize,
        track: Option<TrackSet<K>>,
    },
    SetTracks {
        tracks: [Option<TrackSet<K>>; MAX_TRACKS],
    },
    StopTrack(usize),
    StopAllTracks,
}

pub struct Mixer<K: 'static + Clone + Send + Eq + Hash> {
    #[cfg(not(target_arch = "wasm32"))]
    sender: Sender<AudioCmd<K>>,

    #[cfg(target_arch = "wasm32")]
    speaker: Speaker<K>,

    _thread: Option<JoinHandle<()>>,
    initialized: bool,
    feedback_buffer: Arc<Mutex<Vec<Feedback<K>>>>,
}

impl<K: 'static + Clone + Send + Eq + Hash> Drop for Mixer<K> {
    fn drop(&mut self) {
        if self._thread.is_some() {
            self.unchecked_send(AudioCmd::Quit);

            let thread = self._thread.take().unwrap();
            thread.join().unwrap();
        }
    }
}

impl<K: 'static + Clone + Send + Eq + Hash> Mixer<K> {
    pub fn new() -> Self {
        let feedback_buffer = Arc::new(Mutex::new(Vec::new()));
        let feedback_buffer_ref = Arc::clone(&feedback_buffer);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sender, receiver) = mpsc::channel();

            let _thread = {
                let thread = std::thread::spawn(move || {
                    let mut speaker = Speaker::new(receiver, feedback_buffer_ref);
                    while speaker.listen() {}
                });
                Some(thread)
            };

            Mixer {
                sender,
                _thread,
                initialized: false,
                feedback_buffer,
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let speaker = Speaker::new(feedback_buffer_ref);
            Mixer {
                speaker,
                _thread: None,
                initialized: false,
                feedback_buffer,
            }
        }
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }

    pub fn init(&mut self) {
        if !self.initialized {
            self.send(AudioCmd::Prewarm);
            self.initialized = true;
        }
    }

    pub fn quit(self) {}

    pub fn set_master_volumes(&mut self, track: f32, sound: f32) {
        self.send(AudioCmd::SetMasterVolume {
            track: Some(track),
            sound: Some(sound),
        });
    }

    pub fn set_track_volume(&mut self, volume: f32) {
        self.send(AudioCmd::SetMasterVolume {
            track: Some(volume),
            sound: None,
        });
    }

    pub fn set_sound_volume(&mut self, volume: f32) {
        self.send(AudioCmd::SetMasterVolume {
            track: None,
            sound: Some(volume),
        });
    }

    pub fn set_track(&mut self, index: usize, track: Option<TrackSet<K>>) {
        self.send(AudioCmd::SetTrack { index, track })
    }

    pub fn set_tracks<I>(&mut self, tracks: I)
    where
        I: IntoIterator<Item = TrackSet<K>>,
    {
        let mut all_tracks = [
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None,
        ];
        for (i, track) in tracks.into_iter().enumerate() {
            all_tracks[i] = Some(track);
        }
        self.send(AudioCmd::SetTracks { tracks: all_tracks })
    }

    pub fn stop_track(&mut self, track: usize) {
        self.send(AudioCmd::StopTrack(track));
    }

    pub fn stop_tracks(&mut self) {
        self.send(AudioCmd::StopAllTracks);
    }

    pub fn play_sound(&mut self, sound: Sound<K>) {
        self.send(AudioCmd::PlaySound(sound));
    }

    pub fn feedback(&mut self) -> impl Iterator<Item = Feedback<K>> {
        let items = {
            let indiana_jones = Vec::new();
            let mut buffer = self.feedback_buffer.lock().unwrap();
            std::mem::replace(&mut *buffer, indiana_jones)
        };

        items.into_iter()
    }

    pub fn load_audio(&mut self, key: K, audio_bytes: Cow<'static, [u8]>, volume: Option<f32>) {
        self.unchecked_send(AudioCmd::LoadAudio(key, audio_bytes, volume));
    }

    fn send(&mut self, cmd: AudioCmd<K>) {
        assert!(
            self.initialized || matches!(cmd, AudioCmd::Prewarm),
            "Mixer must have `init()` called before playing sound"
        );
        self.unchecked_send(cmd);
    }

    fn unchecked_send(&mut self, cmd: AudioCmd<K>) {
        #[cfg(not(target_arch = "wasm32"))]
        if self._thread.is_some() {
            self.sender.send(cmd).unwrap();
        }

        #[cfg(target_arch = "wasm32")]
        self.speaker.process(cmd);
    }
}

struct TrackState<K: Clone> {
    track_set: Option<TrackSet<K>>,
    on_hint: bool,
}

impl<K: Clone> TrackState<K> {
    pub fn new() -> TrackState<K> {
        TrackState {
            track_set: None,
            on_hint: false,
        }
    }
}

struct Speaker<K: Clone + Send + Eq + Hash> {
    #[cfg(not(target_arch = "wasm32"))]
    receiver: Receiver<AudioCmd<K>>,

    context: Option<(OutputStream, OutputStreamHandle)>,
    sound_volume: f32,
    track_volume: f32,
    library: HashMap<K, Cow<'static, [u8]>>,
    volumes: HashMap<K, f32>,
    tracks: [TrackState<K>; MAX_TRACKS],
    sinks: [Option<Sink>; MAX_TRACKS],
    feedback_buffer: Arc<Mutex<Vec<Feedback<K>>>>,
}

impl<K: Clone + Send + Eq + Hash + 'static> Speaker<K> {
    pub fn new(
        #[cfg(not(target_arch = "wasm32"))] receiver: Receiver<AudioCmd<K>>,
        feedback_buffer: Arc<Mutex<Vec<Feedback<K>>>>,
    ) -> Self {
        Speaker {
            #[cfg(not(target_arch = "wasm32"))]
            receiver,
            context: None,
            sound_volume: 1.0,
            track_volume: 1.0,
            library: HashMap::default(),
            volumes: HashMap::default(),
            tracks: [
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
                TrackState::new(),
            ],
            sinks: [
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None,
            ],
            feedback_buffer,
        }
    }

    fn warm(&mut self) {
        if self.context.is_none() {
            let context = OutputStream::try_default().unwrap();
            self.context = Some(context);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn listen(&mut self) -> bool {
        let cmd = self.receiver.recv().unwrap();
        self.process(cmd)
    }

    pub fn process(&mut self, cmd: AudioCmd<K>) -> bool {
        match cmd {
            AudioCmd::Quit => return false,
            AudioCmd::Prewarm => self.warm(),
            AudioCmd::LoadAudio(key, audio_bytes, volume) => {
                self.library.insert(key.clone(), audio_bytes);
                if let Some(volume) = volume {
                    self.volumes.insert(key, volume);
                }
            }
            AudioCmd::PlaySound(sound) => self.play_sound(&sound),
            AudioCmd::SetMasterVolume { track, sound } => {
                if let Some(sound) = sound {
                    self.sound_volume = sound;
                }
                if let Some(track) = track {
                    self.track_volume = track;
                }
            }
            AudioCmd::SetTrack { index, track } => {
                self.update_track(index, track);
            }
            AudioCmd::SetTracks { tracks } => {
                for (i, track) in tracks.into_iter().enumerate() {
                    self.update_track(i, track);
                }
            }
            AudioCmd::StopTrack(index) => {
                self.tracks[index] = TrackState::new();
                self.sinks[index] = None;
            }
            AudioCmd::StopAllTracks => {
                for index in 0..MAX_TRACKS {
                    self.tracks[index] = TrackState::new();
                    self.sinks[index] = None;
                }
            }
        }
        true
    }

    fn play_sound(&self, sound: &Sound<K>) {
        let sound_specific_volume = *self.volumes.get(&sound.key).unwrap_or(&1.0);
        let volume = sound_specific_volume * self.sound_volume * sound.volume;

        let audio_bytes = self.library.get(&sound.key);
        if let Some(audio_bytes) = audio_bytes {
            let cursor = Cursor::new(audio_bytes.clone());
            let source = Decoder::new(cursor)
                .unwrap()
                .amplify(volume)
                .speed(sound.speed)
                .convert_samples();
            if let Some((_, handle)) = self.context.as_ref() {
                handle.play_raw(source).unwrap();
            }
        }
    }

    fn update_track(&mut self, index: usize, track: Option<TrackSet<K>>) {
        match (&self.tracks[index].track_set, &track) {
            (None, None) => (),
            (Some(_), None) => {
                self.sinks[index] = None;
            }
            (None, Some(track_set)) => {
                self.sinks[index] = self.create_sink(track_set, index);
            }
            (Some(old), Some(new)) => {
                // Update whether we're on hint or not
                if !self.tracks[index].on_hint && old.hint.is_some() && !old.track.settings.looping
                {
                    if self.sinks[index].as_ref().unwrap().len() < 2 {
                        self.tracks[index].on_hint = true;

                        // TODO: We are only reporting this if
                        // there's a hint - we should probably
                        // always do it when a track ends.
                        let mut buffer = self.feedback_buffer.lock().unwrap();
                        buffer.push(Feedback {
                            track: index,
                            key: old.track.key.clone(),
                            data: FeedbackData::Ended,
                        });
                    }
                }

                let new_hint_track = self.tracks[index].on_hint == true
                    && (old.hint_id().unwrap() == new.track.id());

                let unchanged = old.track.id() == new.track.id() && old.hint_id() == new.hint_id();

                if new_hint_track {
                    self.tracks[index].on_hint = false;
                    if !new.track.settings.looping {
                        if let Some(track) = &new.hint {
                            let sink = self.sinks[index].as_mut().unwrap();
                            let audio_bytes = self
                                .library
                                .get(&track.key)
                                .expect("Failed to look up audio for given key");
                            let cursor = Cursor::new(audio_bytes.clone());
                            let source = Decoder::new(cursor).unwrap();
                            match track.settings.feedback_rate {
                                Some(rate) => {
                                    let feedback_buffer = Arc::clone(&self.feedback_buffer);
                                    let key = track.key.clone();
                                    sink.append(source.periodic_access(rate, move |_| {
                                        let mut buffer = feedback_buffer.lock().unwrap();
                                        buffer.push(Feedback {
                                            track: index,
                                            key: key.clone(),
                                            data: FeedbackData::Tick,
                                        });
                                    }));
                                }
                                None => sink.append(source),
                            }
                        }
                    }
                }

                if new_hint_track || unchanged {
                    let on_hint = self.tracks[index].on_hint;
                    let track = match on_hint {
                        false => &new.track,
                        true => &new.hint.as_ref().unwrap(),
                    };

                    if track.settings.looping {
                        self.keep_sink_looping(track, index);
                    }

                    let sink = self.sinks[index].as_mut().unwrap();

                    if track.settings.playing {
                        sink.play();
                    } else {
                        sink.pause();
                    }

                    let track_specific_volume = *self.volumes.get(&track.key).unwrap_or(&1.0);
                    let volume = track_specific_volume * self.track_volume * track.settings.volume;
                    sink.set_volume(volume);
                } else {
                    self.sinks[index] = self.create_sink(new, index);
                }
            }
        }

        self.tracks[index].track_set = track;
    }

    fn create_sink(&self, track_set: &TrackSet<K>, sink_index: usize) -> Option<Sink> {
        let track_specific_volume = *self.volumes.get(&track_set.track.key).unwrap_or(&1.0);
        let volume = track_specific_volume * self.track_volume * track_set.track.settings.volume;

        let audio_bytes = self
            .library
            .get(&track_set.track.key)
            .expect("Failed to look up audio for given key");

        if let Some((_, handle)) = self.context.as_ref() {
            let sink = Sink::try_new(handle).unwrap();
            sink.set_volume(volume);
            if !track_set.track.settings.playing {
                sink.pause();
            }

            let duplicate_count = match track_set.track.settings.looping {
                true => 2, // We keep second copy in the buffer at all times
                false => 1,
            };

            for _ in 0..duplicate_count {
                let cursor = Cursor::new(audio_bytes.clone());
                let source = Decoder::new(cursor).unwrap();
                match track_set.track.settings.feedback_rate {
                    Some(rate) => {
                        let feedback_buffer = Arc::clone(&self.feedback_buffer);
                        let key = track_set.track.key.clone();
                        sink.append(source.periodic_access(rate, move |_| {
                            let mut buffer = feedback_buffer.lock().unwrap();
                            buffer.push(Feedback {
                                track: sink_index,
                                key: key.clone(),
                                data: FeedbackData::Tick,
                            });
                        }));
                    }
                    None => sink.append(source),
                }
            }

            if !track_set.track.settings.looping {
                if let Some(track) = &track_set.hint {
                    let audio_bytes = self
                        .library
                        .get(&track.key)
                        .expect("Failed to look up audio for given key");
                    let cursor = Cursor::new(audio_bytes.clone());
                    let source = Decoder::new(cursor).unwrap();
                    match track.settings.feedback_rate {
                        Some(rate) => {
                            let feedback_buffer = Arc::clone(&self.feedback_buffer);
                            let key = track.key.clone();
                            sink.append(source.periodic_access(rate, move |_| {
                                let mut buffer = feedback_buffer.lock().unwrap();
                                buffer.push(Feedback {
                                    track: sink_index,
                                    key: key.clone(),
                                    data: FeedbackData::Tick,
                                });
                            }));
                        }
                        None => sink.append(source),
                    }
                }
            }

            return Some(sink);
        }

        None
    }

    fn keep_sink_looping(&mut self, track: &Track<K>, sink_index: usize) {
        let sink = self.sinks[sink_index].as_mut().unwrap();
        let audio_bytes = self
            .library
            .get(&track.key)
            .expect("Failed to look up audio for given key");

        let mut looped = false;

        while sink.len() < 2 {
            let cursor = Cursor::new(audio_bytes.clone());
            let source = Decoder::new(cursor).unwrap();
            sink.append(source);
            looped = true;
        }

        if looped {
            let mut buffer = self.feedback_buffer.lock().unwrap();
            buffer.push(Feedback {
                track: sink_index,
                key: track.key.clone(),
                data: FeedbackData::Looped,
            });
        }
    }
}

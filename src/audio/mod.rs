use dashi::utils::{Handle, Pool};
use tracing::info;

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub enum AudioBackend {
    #[default]
    Dummy,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioEngineInfo {
    pub sample_rate: u32,
    pub channels: u32,
    pub backend: AudioBackend,
}

impl Default for AudioEngineInfo {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            backend: AudioBackend::Dummy,
        }
    }
}

#[repr(C)]
#[allow(dead_code)]
pub struct AudioEngine {
    info: AudioEngineInfo,
    sources: Pool<AudioSource>,
}

impl AudioEngine {
    pub fn new(info: &AudioEngineInfo) -> Self {
        info!(
            "Initializing Audio Engine: {} Hz, {} channels",
            info.sample_rate, info.channels
        );
        Self {
            info: *info,
            sources: Default::default(),
        }
    }

    pub fn create_source(&mut self, path: &str) -> Handle<AudioSource> {
        self.sources
            .insert(AudioSource::new(path))
            .unwrap_or_default()
    }

    pub fn destroy_source(&mut self, h: Handle<AudioSource>) {
        self.sources.release(h);
    }

    fn get_source_mut(&mut self, h: Handle<AudioSource>) -> Option<&mut AudioSource> {
        self.sources.get_mut_ref(h)
    }

    pub fn play(&mut self, h: Handle<AudioSource>) {
        if let Some(s) = self.get_source_mut(h) {
            s.state = PlaybackState::Playing;
        }
    }

    pub fn pause(&mut self, h: Handle<AudioSource>) {
        if let Some(s) = self.get_source_mut(h) {
            s.state = PlaybackState::Paused;
        }
    }

    pub fn stop(&mut self, h: Handle<AudioSource>) {
        if let Some(s) = self.get_source_mut(h) {
            s.state = PlaybackState::Stopped;
        }
    }

    pub fn set_looping(&mut self, h: Handle<AudioSource>, looping: bool) {
        if let Some(s) = self.get_source_mut(h) {
            s.looping = looping;
        }
    }

    pub fn set_volume(&mut self, h: Handle<AudioSource>, volume: f32) {
        if let Some(s) = self.get_source_mut(h) {
            s.volume = volume;
        }
    }

    pub fn set_pitch(&mut self, h: Handle<AudioSource>, pitch: f32) {
        if let Some(s) = self.get_source_mut(h) {
            s.pitch = pitch;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[repr(C)]
pub struct AudioSource {
    #[allow(dead_code)]
    path: String,
    looping: bool,
    volume: f32,
    pitch: f32,
    state: PlaybackState,
}

impl AudioSource {
    fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            looping: false,
            volume: 1.0,
            pitch: 1.0,
            state: PlaybackState::Stopped,
        }
    }
}

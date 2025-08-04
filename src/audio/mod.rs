use dashi::utils::{Handle, Pool};
use glam::{Mat4, Vec3};

use std::{collections::VecDeque, fs::File, io::Read};
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
    listener_transform: Mat4,
    listener_velocity: Vec3,
    sources: Pool<AudioSource>,
    streams: Pool<StreamingSource>,
}

impl AudioEngine {
    pub fn new(info: &AudioEngineInfo) -> Self {
        info!(
            "Initializing Audio Engine: {} Hz, {} channels",
            info.sample_rate, info.channels
        );
        Self {
            info: *info,
            listener_transform: Mat4::IDENTITY,
            listener_velocity: Vec3::ZERO,
            sources: Default::default(),
            streams: Default::default(),
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

    pub fn get_state(&self, h: Handle<AudioSource>) -> Option<PlaybackState> {
        self.sources.get_ref(h).map(|s| s.state)
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

    pub fn set_source_transform(
        &mut self,
        h: Handle<AudioSource>,
        transform: &Mat4,
        velocity: Vec3,
    ) {
        if let Some(s) = self.get_source_mut(h) {
            s.transform = *transform;
            s.velocity = velocity;
        }
    }

    pub fn set_listener_transform(&mut self, transform: &Mat4, velocity: Vec3) {
        self.listener_transform = *transform;
        self.listener_velocity = velocity;
    }

    pub fn create_stream(&mut self, path: &str) -> Handle<StreamingSource> {
        if let Some(stream) = StreamingSource::new(path) {
            self.streams.insert(stream).unwrap_or_default()
        } else {
            Handle::default()
        }
    }

    pub fn update_stream(&mut self, h: Handle<StreamingSource>, out: &mut [u8]) -> usize {
        if let Some(stream) = self.streams.get_mut_ref(h) {
            stream.pop_into(out)
        } else {
            0
        }
    }

    pub fn update(&mut self, _dt: f32) {
        self.streams.for_each_occupied_mut(|s| s.refill());
        self.mix();
    }

    fn mix(&mut self) {
        let listener_pos = self.listener_transform.transform_point3(Vec3::ZERO);
        let listener_vel = self.listener_velocity;

        self.sources.for_each_occupied_mut(|s| {
            let src_pos = s.transform.transform_point3(Vec3::ZERO);
            let dir = listener_pos - src_pos;
            let dist = dir.length();
            let dir_norm = if dist > 0.0 { dir / dist } else { Vec3::ZERO };

            // Simple inverse-distance attenuation.
            let attenuation = 1.0 / (1.0 + dist);
            s.effective_volume = s.volume * attenuation;

            // Doppler effect using the relative velocity along the line-of-sight.
            let rel_vel = (s.velocity - listener_vel).dot(dir_norm);
            const SPEED_OF_SOUND: f32 = 343.0; // meters per second
            let denom = SPEED_OF_SOUND - rel_vel;
            let doppler = if denom.abs() > f32::EPSILON {
                (SPEED_OF_SOUND) / denom
            } else {
                1.0
            };
            s.effective_pitch = s.pitch * doppler;
        });
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
    transform: Mat4,
    velocity: Vec3,
    effective_volume: f32,
    effective_pitch: f32,
}

impl AudioSource {
    fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            looping: false,
            volume: 1.0,
            pitch: 1.0,
            state: PlaybackState::Stopped,
            transform: Mat4::IDENTITY,
            velocity: Vec3::ZERO,
            effective_volume: 1.0,
            effective_pitch: 1.0,
        }
    }
}

const STREAM_CHUNK: usize = 4096;

pub struct StreamingSource {
    #[allow(dead_code)]
    path: String,
    file: File,
    buffer: VecDeque<u8>,
    finished: bool,
}

impl StreamingSource {
    fn new(path: &str) -> Option<Self> {
        let file = File::open(path).ok()?;
        Some(Self {
            path: path.to_string(),
            file,
            buffer: VecDeque::new(),
            finished: false,
        })
    }

    fn refill(&mut self) {
        if self.finished {
            return;
        }
        let mut temp = [0u8; STREAM_CHUNK];
        match self.file.read(&mut temp) {
            Ok(0) => {
                self.finished = true;
            }
            Ok(n) => {
                self.buffer.extend(&temp[..n]);
            }
            Err(_) => {
                self.finished = true;
            }
        }
    }

    fn pop_into(&mut self, out: &mut [u8]) -> usize {
        let mut count = 0;
        while count < out.len() {
            if let Some(v) = self.buffer.pop_front() {
                out[count] = v;
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

use dashi::utils::{Handle, Pool};
use glam::{Mat4, Vec3};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::{
    collections::{HashMap, VecDeque},
    ffi::c_void,
    fs::File,
    io::{BufReader, Read},
};
use tracing::info;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub enum AudioBackend {
    #[default]
    Dummy,
    Cpal,
    Rodio,
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
pub struct Bus {
    volume: f32,
    parent: Option<Handle<Bus>>,
}

impl Bus {
    fn new(parent: Option<Handle<Bus>>) -> Self {
        Self {
            volume: 1.0,
            parent,
        }
    }
}

pub type BusHandle = Handle<Bus>;
pub type FinishedCallback = extern "C" fn(Handle<AudioSource>, *mut c_void);

#[repr(C)]
#[allow(dead_code)]
pub struct AudioEngine {
    info: AudioEngineInfo,
    listener_transform: Mat4,
    listener_velocity: Vec3,
    sources: Pool<AudioSource>,
    streams: Pool<StreamingSource>,
    buses: Pool<Bus>,
    master_bus: Handle<Bus>,
    music_bus: Handle<Bus>,
    effects_bus: Handle<Bus>,
    finished_callbacks: Vec<(FinishedCallback, *mut c_void)>,
    rodio: Option<RodioContext>,
    sinks: HashMap<Handle<AudioSource>, Sink>,
}

struct RodioContext {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

impl AudioEngine {
    pub fn new(info: &AudioEngineInfo) -> Self {
        info!(
            "Initializing Audio Engine: {} Hz, {} channels",
            info.sample_rate, info.channels
        );
        let mut buses = Pool::default();
        let master_bus = buses.insert(Bus::new(None)).unwrap_or_default();
        let music_bus = buses.insert(Bus::new(Some(master_bus))).unwrap_or_default();
        let effects_bus = buses.insert(Bus::new(Some(master_bus))).unwrap_or_default();

        let mut info_copy = *info;
        let rodio = if info.backend == AudioBackend::Rodio {
            match OutputStream::try_default() {
                Ok((stream, handle)) => Some(RodioContext {
                    _stream: stream,
                    handle,
                }),
                Err(_) => {
                    info_copy.backend = AudioBackend::Dummy;
                    None
                }
            }
        } else {
            None
        };

        Self {
            info: info_copy,
            listener_transform: Mat4::IDENTITY,
            listener_velocity: Vec3::ZERO,
            sources: Default::default(),
            streams: Default::default(),
            buses,
            master_bus,
            music_bus,
            effects_bus,
            finished_callbacks: Vec::new(),
            rodio,
            sinks: HashMap::new(),
        }
    }

    pub fn create_source(&mut self, path: &str) -> Handle<AudioSource> {
        self.sources
            .insert(AudioSource::new(path, self.effects_bus))
            .unwrap_or_default()
    }

    pub fn backend(&self) -> AudioBackend {
        self.info.backend
    }

    pub fn destroy_source(&mut self, h: Handle<AudioSource>) {
        if let Some(sink) = self.sinks.remove(&h) {
            sink.stop();
        }
        self.sources.release(h);
    }

    fn get_source_mut(&mut self, h: Handle<AudioSource>) -> Option<&mut AudioSource> {
        self.sources.get_mut_ref(h)
    }

    pub fn get_state(&self, h: Handle<AudioSource>) -> Option<PlaybackState> {
        self.sources.get_ref(h).map(|s| s.state)
    }

    pub fn play(&mut self, h: Handle<AudioSource>) {
        let rodio_handle = if self.info.backend == AudioBackend::Rodio {
            self.rodio.as_ref().map(|ctx| ctx.handle.clone())
        } else {
            None
        };

        let mut path = String::new();
        let mut volume = 1.0f32;
        let mut pitch = 1.0f32;
        let has_source = if let Some(s) = self.get_source_mut(h) {
            s.state = PlaybackState::Playing;
            path = s.path.clone();
            volume = s.volume;
            pitch = s.pitch;
            true
        } else {
            false
        };
        if !has_source {
            return;
        }

        if let Some(handle) = rodio_handle {
            if !self.sinks.contains_key(&h) {
                if let Ok(file) = File::open(&path) {
                    if let Ok(decoder) = Decoder::new(BufReader::new(file)) {
                        if let Ok(sink) = Sink::try_new(&handle) {
                            sink.append(decoder);
                            sink.set_volume(volume);
                            sink.set_speed(pitch);
                            self.sinks.insert(h, sink);
                        }
                    }
                }
            } else if let Some(sink) = self.sinks.get(&h) {
                sink.play();
            }
        }
    }

    pub fn pause(&mut self, h: Handle<AudioSource>) {
        {
            if let Some(s) = self.get_source_mut(h) {
                s.state = PlaybackState::Paused;
            }
        }
        if let Some(sink) = self.sinks.get(&h) {
            sink.pause();
        }
    }

    pub fn stop(&mut self, h: Handle<AudioSource>) {
        let was_playing = {
            if let Some(s) = self.get_source_mut(h) {
                let wp = s.state == PlaybackState::Playing;
                s.state = PlaybackState::Stopped;
                wp
            } else {
                false
            }
        };
        if let Some(sink) = self.sinks.remove(&h) {
            sink.stop();
        }
        if was_playing {
            self.notify_finished(h);
        }
    }

    pub fn set_looping(&mut self, h: Handle<AudioSource>, looping: bool) {
        if let Some(s) = self.get_source_mut(h) {
            s.looping = looping;
        }
    }

    pub fn set_volume(&mut self, h: Handle<AudioSource>, volume: f32) {
        {
            if let Some(s) = self.get_source_mut(h) {
                s.volume = volume;
            }
        }
        if let Some(sink) = self.sinks.get(&h) {
            sink.set_volume(volume);
        }
    }

    pub fn set_pitch(&mut self, h: Handle<AudioSource>, pitch: f32) {
        {
            if let Some(s) = self.get_source_mut(h) {
                s.pitch = pitch;
            }
        }
        if let Some(sink) = self.sinks.get(&h) {
            sink.set_speed(pitch);
        }
    }

    pub fn set_bus_volume(&mut self, h: Handle<Bus>, volume: f32) {
        if let Some(b) = self.buses.get_mut_ref(h) {
            b.volume = volume;
        }
    }

    pub fn register_finished_callback(&mut self, cb: FinishedCallback, user_data: *mut c_void) {
        self.finished_callbacks.push((cb, user_data));
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
        let buses_ptr: *const Pool<Bus> = &self.buses;
        let mut handles = Vec::new();
        self.sources
            .for_each_occupied_handle_mut(|h| handles.push(h));
        for h in handles {
            if let Some(s) = self.sources.get_mut_ref(h) {
                let src_pos = s.transform.transform_point3(Vec3::ZERO);
                let dir = listener_pos - src_pos;
                let dist = dir.length();
                let dir_norm = if dist > 0.0 { dir / dist } else { Vec3::ZERO };

                // Simple inverse-distance attenuation.
                let attenuation = 1.0 / (1.0 + dist);
                let bus_volume = unsafe { compute_bus_volume(&*buses_ptr, s.bus) };
                s.effective_volume = s.volume * bus_volume * attenuation;

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

                if let Some(sink) = self.sinks.get(&h) {
                    sink.set_volume(s.effective_volume);
                    sink.set_speed(s.effective_pitch);
                    match s.state {
                        PlaybackState::Playing => sink.play(),
                        PlaybackState::Paused => sink.pause(),
                        PlaybackState::Stopped => {}
                    }
                }
            }
        }
    }

    fn notify_finished(&self, h: Handle<AudioSource>) {
        for (cb, data) in &self.finished_callbacks {
            cb(h, *data);
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
    transform: Mat4,
    velocity: Vec3,
    effective_volume: f32,
    effective_pitch: f32,
    bus: Handle<Bus>,
}

impl AudioSource {
    fn new(path: &str, bus: Handle<Bus>) -> Self {
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
            bus,
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

fn compute_bus_volume(buses: &Pool<Bus>, h: Handle<Bus>) -> f32 {
    if let Some(bus) = buses.get_ref(h) {
        if let Some(parent) = bus.parent {
            bus.volume * compute_bus_volume(buses, parent)
        } else {
            bus.volume
        }
    } else {
        1.0
    }
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

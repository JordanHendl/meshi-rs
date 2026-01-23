use glam::{Mat4, Vec3};
use noren::{rdb::audio::AudioClip, DB};
use resource_pool::{Handle, Pool};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::{BufReader, Cursor, Read, Seek};
use std::mem::MaybeUninit;
use std::{ffi::c_void, ptr::NonNull, sync::Arc};
use tracing::info;

trait AudioReadSeek: Read + Seek + Send + Sync + 'static {}
impl<T: Read + Seek + Send + Sync + 'static> AudioReadSeek for T {}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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
    pub debug_mode: bool,
}

impl Default for AudioEngineInfo {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            backend: AudioBackend::Dummy,
            debug_mode: false,
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
    sources: Pool<AudioSourceSlot>,
    streams: Pool<StreamingSource>,
    buses: Pool<Bus>,
    master_bus: Handle<Bus>,
    music_bus: Handle<Bus>,
    effects_bus: Handle<Bus>,
    finished_callbacks: Vec<(FinishedCallback, *mut c_void)>,
    rodio_stream: Option<OutputStream>,
    rodio_handle: Option<OutputStreamHandle>,
    db: Option<NonNull<DB>>,
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
        let (rodio_stream, rodio_handle) = if info.backend == AudioBackend::Rodio {
            match OutputStream::try_default() {
                Ok((stream, handle)) => (Some(stream), Some(handle)),
                Err(e) => {
                    info!("Failed to initialize Rodio backend: {}", e);
                    info_copy.backend = AudioBackend::Dummy;
                    (None, None)
                }
            }
        } else {
            (None, None)
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
            rodio_stream,
            rodio_handle,
            db: None,
        }
    }

    pub fn debug_mode(&self) -> bool {
        self.info.debug_mode
    }

    pub fn set_debug_mode(&mut self, enabled: bool) {
        self.info.debug_mode = enabled;
    }

    pub fn create_source(&mut self, path: &str) -> Handle<AudioSource> {
        let Some(mut db) = self.db else {
            info!("Audio database unavailable; cannot load clip '{}'", path);
            return Handle::default();
        };

        match unsafe { db.as_mut().audio_mut().fetch_clip(path) } {
            Ok(clip) => self
                .sources
                .insert(AudioSourceSlot::new(AudioSource::new_clip(
                    clip,
                    self.effects_bus,
                )))
                .map(to_public_source_handle)
                .unwrap_or_default(),
            Err(err) => {
                info!("Failed to load audio clip '{}': {:?}", path, err);
                Handle::default()
            }
        }
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("audio db ptr"));
    }

    pub fn backend(&self) -> AudioBackend {
        self.info.backend
    }

    pub fn destroy_source(&mut self, h: Handle<AudioSource>) {
        if self.info.backend == AudioBackend::Rodio {
            if let Some(s) = self.sources.get_mut_ref(to_slot_handle(h)) {
                if let Some(sink) = s.as_mut().sink.take() {
                    sink.stop();
                }
            }
        }
        if let Some(source) = self.sources.get_mut_ref(to_slot_handle(h)) {
            unsafe {
                source.drop_in_place();
            }
        }
        self.sources.release(to_slot_handle(h));
    }

    fn get_source_mut(&mut self, h: Handle<AudioSource>) -> Option<&mut AudioSource> {
        self.sources
            .get_mut_ref(to_slot_handle(h))
            .map(AudioSourceSlot::as_mut)
    }

    pub fn get_state(&self, h: Handle<AudioSource>) -> Option<PlaybackState> {
        self.sources
            .get_ref(to_slot_handle(h))
            .map(|s| s.as_ref().state)
    }

    pub fn play(&mut self, h: Handle<AudioSource>) {
        let backend = self.info.backend;
        let handle_clone = self.rodio_handle.clone();
        if let Some(s) = self.get_source_mut(h) {
            if backend == AudioBackend::Rodio {
                if let Some(handle) = handle_clone {
                    let reader: Option<Box<dyn AudioReadSeek>> = match &s.source {
                        AudioSourceData::Clip { data, .. } => {
                            Some(Box::new(Cursor::new(Arc::clone(data))) as Box<dyn AudioReadSeek>)
                        }
                    };
                    if let Some(reader) = reader {
                        let decoder = Decoder::new(BufReader::new(reader)).ok();
                        let Some(decoder) = decoder else {
                            s.state = PlaybackState::Playing;
                            return;
                        };
                        if let Ok(sink) = Sink::try_new(&handle) {
                            if s.looping {
                                sink.append(decoder.repeat_infinite());
                            } else {
                                sink.append(decoder);
                            }
                            sink.set_volume(s.volume);
                            sink.play();
                            s.sink = Some(sink);
                            s.state = PlaybackState::Playing;
                            return;
                        }
                    }
                }
            }
            s.state = PlaybackState::Playing;
        }
    }

    pub fn pause(&mut self, h: Handle<AudioSource>) {
        let backend = self.info.backend;
        if let Some(s) = self.get_source_mut(h) {
            if backend == AudioBackend::Rodio {
                if let Some(sink) = &s.sink {
                    sink.pause();
                }
            }
            s.state = PlaybackState::Paused;
        }
    }

    pub fn stop(&mut self, h: Handle<AudioSource>) {
        let backend = self.info.backend;
        if let Some(s) = self.get_source_mut(h) {
            if backend == AudioBackend::Rodio {
                if let Some(sink) = s.sink.take() {
                    sink.stop();
                }
            }
            let was_playing = s.state == PlaybackState::Playing;
            s.state = PlaybackState::Stopped;
            if was_playing {
                self.notify_finished(h);
            }
        }
    }

    pub fn set_looping(&mut self, h: Handle<AudioSource>, looping: bool) {
        if let Some(s) = self.get_source_mut(h) {
            s.looping = looping;
        }
    }

    pub fn set_volume(&mut self, h: Handle<AudioSource>, volume: f32) {
        let backend = self.info.backend;
        if let Some(s) = self.get_source_mut(h) {
            s.volume = volume;
            if backend == AudioBackend::Rodio {
                if let Some(sink) = &s.sink {
                    sink.set_volume(volume);
                }
            }
        }
    }

    pub fn set_pitch(&mut self, h: Handle<AudioSource>, pitch: f32) {
        if let Some(s) = self.get_source_mut(h) {
            s.pitch = pitch;
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
        let Some(mut db) = self.db else {
            info!("Audio database unavailable; cannot load stream '{}'", path);
            return Handle::default();
        };

        match unsafe { db.as_mut().audio_mut().fetch_clip(path) } {
            Ok(clip) => self
                .streams
                .insert(StreamingSource::new_clip(clip))
                .unwrap_or_default(),
            Err(err) => {
                info!("Failed to load audio stream '{}': {:?}", path, err);
                Handle::default()
            }
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
        self.mix();
    }

    fn mix(&mut self) {
        let listener_pos = self.listener_transform.transform_point3(Vec3::ZERO);
        let listener_vel = self.listener_velocity;
        let buses_ptr: *const Pool<Bus> = &self.buses;
        self.sources.for_each_occupied_mut(|slot| {
            let s = slot.as_mut();
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
        });
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
    source: AudioSourceData,
    looping: bool,
    volume: f32,
    pitch: f32,
    state: PlaybackState,
    transform: Mat4,
    velocity: Vec3,
    effective_volume: f32,
    effective_pitch: f32,
    bus: Handle<Bus>,
    sink: Option<Sink>,
}

struct AudioSourceSlot {
    source: MaybeUninit<AudioSource>,
}

impl AudioSourceSlot {
    fn new(source: AudioSource) -> Self {
        Self {
            source: MaybeUninit::new(source),
        }
    }

    fn as_ref(&self) -> &AudioSource {
        unsafe { self.source.assume_init_ref() }
    }

    fn as_mut(&mut self) -> &mut AudioSource {
        unsafe { self.source.assume_init_mut() }
    }

    unsafe fn drop_in_place(&mut self) {
        std::ptr::drop_in_place(self.source.as_mut_ptr());
    }
}

#[derive(Debug, Clone)]
enum AudioSourceData {
    Clip { name: String, data: Arc<[u8]> },
}

impl AudioSource {
    fn new_clip(clip: AudioClip, bus: Handle<Bus>) -> Self {
        Self {
            source: AudioSourceData::Clip {
                name: clip.name,
                data: Arc::from(clip.data.into_boxed_slice()),
            },
            looping: false,
            volume: 1.0,
            pitch: 1.0,
            state: PlaybackState::Stopped,
            transform: Mat4::IDENTITY,
            velocity: Vec3::ZERO,
            effective_volume: 1.0,
            effective_pitch: 1.0,
            bus,
            sink: None,
        }
    }
}

fn to_slot_handle(handle: Handle<AudioSource>) -> Handle<AudioSourceSlot> {
    Handle::new(handle.slot, handle.generation)
}

fn to_public_source_handle(handle: Handle<AudioSourceSlot>) -> Handle<AudioSource> {
    Handle::new(handle.slot, handle.generation)
}

pub struct StreamingSource {
    #[allow(dead_code)]
    name: String,
    data: Arc<[u8]>,
    cursor: usize,
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
    fn new_clip(clip: AudioClip) -> Self {
        Self {
            name: clip.name,
            data: Arc::from(clip.data.into_boxed_slice()),
            cursor: 0,
        }
    }

    fn pop_into(&mut self, out: &mut [u8]) -> usize {
        let mut count = 0;
        while count < out.len() {
            let Some(v) = self.data.get(self.cursor) else {
                break;
            };
            out[count] = *v;
            count += 1;
            self.cursor += 1;
        }
        count
    }
}

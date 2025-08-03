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
}

impl AudioEngine {
    pub fn new(info: &AudioEngineInfo) -> Self {
        info!(
            "Initializing Audio Engine: {} Hz, {} channels",
            info.sample_rate, info.channels
        );
        Self { info: *info }
    }
}

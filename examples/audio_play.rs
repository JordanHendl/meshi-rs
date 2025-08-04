use meshi::audio::{AudioBackend, AudioEngine, AudioEngineInfo};
use std::{path::Path, thread::sleep, time::Duration};

fn main() {
    tracing_subscriber::fmt::init();
    let info = AudioEngineInfo {
        backend: AudioBackend::Rodio,
        ..Default::default()
    };
    let mut engine = AudioEngine::new(&info);
    if engine.backend() != AudioBackend::Rodio {
        eprintln!("Rodio backend not available");
        return;
    }
    let path = if cfg!(target_os = "windows") {
        "C:\\Windows\\Media\\tada.wav"
    } else if cfg!(target_os = "linux") {
        "/usr/share/sounds/alsa/Front_Center.wav"
    } else {
        ""
    };
    if path.is_empty() || !Path::new(path).exists() {
        eprintln!("System audio file not found; skipping playback");
        return;
    }
    let src = engine.create_source(path);
    engine.play(src);
    for _ in 0..10 {
        engine.update(0.1);
        sleep(Duration::from_millis(100));
    }
}

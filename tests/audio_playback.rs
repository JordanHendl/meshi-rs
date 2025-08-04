use meshi::audio::{AudioBackend, AudioEngine, AudioEngineInfo, PlaybackState};
use std::{path::Path, thread::sleep, time::Duration};

#[test]
fn rodio_backend_plays_sound() {
    let info = AudioEngineInfo {
        backend: AudioBackend::Rodio,
        ..Default::default()
    };
    let mut engine = AudioEngine::new(&info);
    if engine.backend() != AudioBackend::Rodio {
        // Skip test if rodio backend couldn't be initialized
        return;
    }
    let path = if cfg!(target_os = "windows") {
        "C:\\Windows\\Media\\tada.wav"
    } else if cfg!(target_os = "linux") {
        "/usr/share/sounds/alsa/Front_Center.wav"
    } else {
        return;
    };
    if !Path::new(path).exists() {
        // No system audio file available; skip test
        return;
    }
    let h = engine.create_source(path);
    engine.play(h);
    sleep(Duration::from_millis(100));
    engine.update(0.1);
    assert_eq!(engine.get_state(h), Some(PlaybackState::Playing));
    engine.stop(h);
}

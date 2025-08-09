use meshi::audio::{AudioEngine, AudioEngineInfo, PlaybackState};
use meshi::{meshi_audio_get_state, meshi_audio_pause, meshi_audio_play, meshi_audio_stop};
use serial_test::serial;

#[test]
#[serial]
#[ignore]
fn audio_state_transitions() {
    let mut audio = AudioEngine::new(&AudioEngineInfo::default());
    let h = audio.create_source("dummy");

    assert_eq!(meshi_audio_get_state(&mut audio, h), PlaybackState::Stopped);

    meshi_audio_play(&mut audio, h);
    assert_eq!(meshi_audio_get_state(&mut audio, h), PlaybackState::Playing);

    meshi_audio_pause(&mut audio, h);
    assert_eq!(meshi_audio_get_state(&mut audio, h), PlaybackState::Paused);

    meshi_audio_stop(&mut audio, h);
    assert_eq!(meshi_audio_get_state(&mut audio, h), PlaybackState::Stopped);
}

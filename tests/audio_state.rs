use meshi::audio::{AudioEngine, AudioEngineInfo, PlaybackState};

#[test]
fn audio_state_transitions() {
    let mut audio = AudioEngine::new(&AudioEngineInfo::default());
    let h = audio.create_source("dummy");

    assert_eq!(audio.get_state(h), Some(PlaybackState::Stopped));

    audio.play(h);
    assert_eq!(audio.get_state(h), Some(PlaybackState::Playing));

    audio.pause(h);
    assert_eq!(audio.get_state(h), Some(PlaybackState::Paused));

    audio.stop(h);
    assert_eq!(audio.get_state(h), Some(PlaybackState::Stopped));
}

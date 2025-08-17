use meshi::audio::{AudioBackend, AudioEngine, AudioEngineInfo};
use std::{path::Path, thread::sleep, time::Duration};

fn system_sound_path() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        const CANDIDATES: [&str; 2] = [
            "/usr/share/sounds/alsa/Front_Center.wav",
            "/usr/share/sounds/freedesktop/stereo/bell.oga",
        ];
        CANDIDATES.iter().find(|p| Path::new(p).exists()).copied()
    }
    #[cfg(target_os = "macos")]
    {
        const CANDIDATES: [&str; 1] = ["/System/Library/Sounds/Glass.aiff"];
        CANDIDATES.iter().find(|p| Path::new(p).exists()).copied()
    }
    #[cfg(target_os = "windows")]
    {
        const CANDIDATES: [&str; 2] = [
            "C:\\Windows\\Media\\notify.wav",
            "C:\\Windows\\Media\\tada.wav",
        ];
        CANDIDATES.iter().find(|p| Path::new(p).exists()).copied()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn main() {
    if let Some(path) = system_sound_path() {
        let mut audio = AudioEngine::new(&AudioEngineInfo {
            backend: AudioBackend::Rodio,
            ..Default::default()
        });
        if audio.backend() == AudioBackend::Rodio {
            let h = audio.create_source(path);
            audio.play(h);
            sleep(Duration::from_millis(200));
            audio.stop(h);
        } else {
            eprintln!("Rodio backend unavailable");
        }
    } else {
        eprintln!("No system sound found; example skipped");
    }
}

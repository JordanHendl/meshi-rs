use meshi_audio::{AudioBackend, AudioEngine, AudioEngineInfo};
use noren::{
    rdb::audio::{AudioClip, AudioFormat},
    DBInfo, RDBFile, DB,
};
use std::{
    f32::consts::PI,
    fs,
    path::PathBuf,
    thread::sleep,
    time::Duration,
};

const SAMPLE_CLIP: &str = "audio/sample";

fn make_sine_wav(sample_rate: u32, duration_secs: f32, frequency_hz: f32) -> Vec<u8> {
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let total_samples = (sample_rate as f32 * duration_secs) as u32;
    let block_align = num_channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * block_align as u32;
    let data_size = total_samples * block_align as u32;

    let mut out = Vec::with_capacity(44 + data_size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&num_channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());

    let amplitude = i16::MAX as f32 * 0.2;
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * frequency_hz * t).sin() * amplitude;
        out.extend_from_slice(&(sample as i16).to_le_bytes());
    }

    out
}

fn write_sample_database() -> PathBuf {
    let base_dir = std::env::temp_dir().join("meshi_audio_example");
    fs::create_dir_all(&base_dir).expect("create temp audio dir");
    let data = make_sine_wav(48_000, 0.5, 440.0);
    let clip = AudioClip::new(SAMPLE_CLIP.to_string(), AudioFormat::Wav, data);
    let mut rdb = RDBFile::new();
    rdb.add(SAMPLE_CLIP, &clip).expect("add audio clip");
    let audio_path = base_dir.join("audio.rdb");
    rdb.save(&audio_path).expect("save audio database");
    base_dir
}

fn main() {
    tracing_subscriber::fmt::init();
    let base_dir = write_sample_database();
    let mut db = DB::new(&DBInfo {
        base_dir: base_dir.to_str().expect("temp dir path"),
        layout_file: None,
        pooled_geometry_uploads: false,
    })
    .expect("create database");

    let mut audio = AudioEngine::new(&AudioEngineInfo {
        backend: AudioBackend::Rodio,
        ..Default::default()
    });
    audio.initialize_database(&mut db);

    if audio.backend() == AudioBackend::Rodio {
        let h = audio.create_source(SAMPLE_CLIP);
        if h.valid() {
            audio.play(h);
            sleep(Duration::from_millis(400));
            audio.stop(h);
        } else {
            eprintln!("Sample audio clip not found in the database");
        }
    } else {
        eprintln!("Rodio backend unavailable");
    }
}

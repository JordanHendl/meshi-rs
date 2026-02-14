use dashi::{Handle, SampleCount};
use glam::{Mat4, Vec3};
use meshi_graphics::{DisplayInfo, RenderEngine, RenderEngineInfo, RendererSelect, WindowInfo};
use std::{
    fs,
    io::{self, BufRead},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

pub struct RuntimeFrame {
    pub size: [usize; 2],
    pub pixels: Vec<u8>,
}

#[derive(Default)]
pub struct RuntimeControlState {
    pub playing: bool,
    step_requested: bool,
}

impl RuntimeControlState {
    pub fn request_step(&mut self) {
        self.step_requested = true;
    }

    pub fn consume_step(&mut self) -> bool {
        if self.step_requested {
            self.step_requested = false;
            true
        } else {
            false
        }
    }
}

pub struct RuntimeBridge {
    engine: Option<RenderEngine>,
    display: Option<Handle<meshi_graphics::Display>>,
    camera: Option<Handle<meshi_graphics::Camera>>,
    viewport_size: [u32; 2],
    last_frame: Option<RuntimeFrame>,
    template_root: PathBuf,
    project_runtime_root: Option<PathBuf>,
    repo_root: PathBuf,
    status: RuntimeStatus,
    last_error: Option<String>,
    logs: Vec<RuntimeLogEntry>,
    event_rx: Receiver<RuntimeEvent>,
    event_tx: Sender<RuntimeEvent>,
    build_thread: Option<JoinHandle<()>>,
    child: Option<Child>,
}

impl RuntimeBridge {
    pub fn new() -> Self {
        let editor_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let template_root = editor_root.join("templates").join("cpp").join("meshi_app");
        let repo_root = editor_root.parent().unwrap_or(&editor_root).to_path_buf();
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            engine: None,
            display: None,
            camera: None,
            viewport_size: [0, 0],
            last_frame: None,
            template_root,
            project_runtime_root: None,
            repo_root,
            status: RuntimeStatus::Idle,
            last_error: None,
            logs: Vec::new(),
            event_rx,
            event_tx,
            build_thread: None,
            child: None,
        }
    }

    pub fn latest_frame(&self) -> Option<&RuntimeFrame> {
        self.last_frame.as_ref()
    }

    pub fn logs(&self) -> &[RuntimeLogEntry] {
        &self.logs
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn status(&self) -> RuntimeStatus {
        self.status
    }

    pub fn log_message(&mut self, level: RuntimeLogLevel, message: impl Into<String>) {
        self.push_log(level, message);
    }

    pub fn build_project(&mut self, project_root: Option<&Path>) {
        self.start_build(project_root, RuntimeBuildAction::BuildOnly);
    }

    pub fn build_and_run(&mut self, project_root: Option<&Path>) {
        self.start_build(project_root, RuntimeBuildAction::BuildAndRun);
    }

    pub fn rebuild_all(&mut self, project_root: Option<&Path>) {
        self.start_build(project_root, RuntimeBuildAction::RebuildAll);
    }

    pub fn poll(&mut self) {
        self.drain_events();
        self.cleanup_build_thread();
        self.poll_child();
    }

    pub fn tick(
        &mut self,
        delta_time: f32,
        controls: &mut RuntimeControlState,
        viewport_pixels: [u32; 2],
    ) -> bool {
        self.poll();
        let viewport_pixels = [viewport_pixels[0].max(1), viewport_pixels[1].max(1)];
        let mut size_changed = false;
        if self.engine.is_none() || self.viewport_size != viewport_pixels {
            size_changed = true;
            self.recreate_engine(viewport_pixels);
        }

        let should_step = controls.consume_step();
        let should_render = controls.playing || should_step || size_changed;
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };

        if should_render {
            let frame_delta = if controls.playing || should_step {
                delta_time.max(1.0 / 240.0)
            } else {
                0.0
            };
            engine.update(frame_delta);

            let Some(display) = self.display else {
                return false;
            };
            if let Some(frame) = engine.frame_dump(display) {
                let pixel_len = (frame.width as usize)
                    .saturating_mul(frame.height as usize)
                    .saturating_mul(4);
                let src = unsafe { std::slice::from_raw_parts(frame.pixels, pixel_len) };
                let mut pixels = Vec::with_capacity(pixel_len);
                for chunk in src.chunks_exact(4) {
                    pixels.push(chunk[2]);
                    pixels.push(chunk[1]);
                    pixels.push(chunk[0]);
                    pixels.push(chunk[3]);
                }
                self.last_frame = Some(RuntimeFrame {
                    size: [frame.width as usize, frame.height as usize],
                    pixels,
                });
            }
        }
        should_render
    }

    fn recreate_engine(&mut self, viewport_pixels: [u32; 2]) {
        let info = RenderEngineInfo {
            headless: true,
            canvas_extent: Some(viewport_pixels),
            renderer: RendererSelect::Deferred,
            sample_count: Some(SampleCount::S1),
            skybox_cubemap_entry: None,
            debug_mode: false,
            shadow_cascades: Default::default(),
        };
        let mut engine = RenderEngine::new(&info).expect("Failed to create RenderEngine");
        let mut display_info = DisplayInfo::default();
        display_info.window = WindowInfo {
            title: "Meshi Editor Viewport".to_string(),
            size: viewport_pixels,
            resizable: false,
        };
        let display = engine.register_cpu_display(display_info);

        let camera = engine.register_camera(&Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0)));
        engine.set_camera_perspective(
            camera,
            60f32.to_radians(),
            viewport_pixels[0] as f32,
            viewport_pixels[1] as f32,
            0.1,
            2000.0,
        );
        engine.attach_camera_to_display(display, camera);

        self.engine = Some(engine);
        self.display = Some(display);
        self.camera = Some(camera);
        self.viewport_size = viewport_pixels;
    }

    fn start_build(&mut self, project_root: Option<&Path>, action: RuntimeBuildAction) {
        self.cleanup_build_thread();
        if matches!(self.status, RuntimeStatus::Building) {
            self.push_log(RuntimeLogLevel::Warn, "Build already in progress.");
            return;
        }

        self.stop_running_process();
        self.last_error = None;
        self.status = RuntimeStatus::Building;
        self.push_log(RuntimeLogLevel::Info, "Starting C++ build pipeline...");

        let Some(project_root) = project_root.map(PathBuf::from) else {
            self.last_error =
                Some("No active project. Create or open a project first.".to_string());
            self.status = RuntimeStatus::Failed;
            self.push_log(
                RuntimeLogLevel::Error,
                "No active project selected for build.",
            );
            return;
        };
        let runtime_root = project_root.join("apps").join("hello_engine");
        let build_root = runtime_root.join("build");

        self.project_runtime_root = Some(runtime_root.clone());

        let template_root = self.template_root.clone();
        let repo_root = self.repo_root.clone();
        let sender = self.event_tx.clone();

        self.build_thread = Some(thread::spawn(move || {
            if let Err(err) = ensure_runtime_workspace(&template_root, &runtime_root, action) {
                let _ = sender.send(RuntimeEvent::Error(format!(
                    "Failed to prepare runtime workspace: {}",
                    err
                )));
                return;
            }

            let wrapper_dir = match std::env::var("MESHI_WRAPPER_DIR") {
                Ok(path) => PathBuf::from(path),
                Err(_) => {
                    let _ = sender.send(RuntimeEvent::Error(
                        "MESHI_WRAPPER_DIR is not set. Point it at the Meshi C++ wrapper repo."
                            .to_string(),
                    ));
                    return;
                }
            };

            let configure_status = run_command(
                &sender,
                "cmake",
                vec![
                    "-S".into(),
                    runtime_root.display().to_string(),
                    "-B".into(),
                    build_root.display().to_string(),
                    format!("-DMESHI_WRAPPER_DIR={}", wrapper_dir.display()),
                    format!("-DMESHI_RS_DIR={}", repo_root.display()),
                    "-DCMAKE_BUILD_TYPE=Debug".into(),
                ],
            );

            if !configure_status {
                return;
            }

            let mut build_args = vec!["--build".into(), build_root.display().to_string()];
            if cfg!(windows) {
                build_args.push("--config".into());
                build_args.push("Debug".into());
            }

            let build_status = run_command(&sender, "cmake", build_args);
            if !build_status {
                return;
            }

            let _ = sender.send(RuntimeEvent::BuildFinished {
                run: action.run_after(),
            });
        }));
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                RuntimeEvent::Log(entry) => {
                    self.logs.push(entry);
                    if self.logs.len() > 500 {
                        let drain_count = self.logs.len() - 500;
                        self.logs.drain(0..drain_count);
                    }
                }
                RuntimeEvent::Error(message) => {
                    self.last_error = Some(message.clone());
                    self.status = RuntimeStatus::Failed;
                    self.logs.push(RuntimeLogEntry {
                        level: RuntimeLogLevel::Error,
                        message,
                    });
                }
                RuntimeEvent::BuildFinished { run } => {
                    self.status = RuntimeStatus::Idle;
                    if run {
                        self.launch_runtime();
                    }
                }
            }
        }
    }

    fn cleanup_build_thread(&mut self) {
        if let Some(handle) = self.build_thread.take() {
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                self.build_thread = Some(handle);
            }
        }
    }

    fn poll_child(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                self.status = RuntimeStatus::Idle;
                self.push_log(
                    RuntimeLogLevel::Info,
                    format!("Runtime process exited with status {}", status),
                );
                self.child = None;
            }
        }
    }

    fn stop_running_process(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.push_log(RuntimeLogLevel::Warn, "Stopped running runtime process.");
            self.status = RuntimeStatus::Idle;
        }
    }

    fn launch_runtime(&mut self) {
        let Some(runtime_root) = self.project_runtime_root.clone() else {
            self.last_error = Some("No active project runtime configured.".to_string());
            self.status = RuntimeStatus::Failed;
            self.push_log(RuntimeLogLevel::Error, "Runtime project is not configured.");
            return;
        };

        let build_root = runtime_root.join("build");
        let executable = runtime_executable_path(&build_root);
        if !executable.exists() {
            self.last_error = Some(format!(
                "Runtime executable not found at {}",
                executable.display()
            ));
            self.status = RuntimeStatus::Failed;
            self.push_log(RuntimeLogLevel::Error, "Runtime executable missing.");
            return;
        }

        let mut command = Command::new(&executable);
        command.current_dir(&runtime_root);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        match command.spawn() {
            Ok(mut child) => {
                if let Some(stdout) = child.stdout.take() {
                    spawn_output_reader(stdout, self.event_tx.clone(), RuntimeLogLevel::Info);
                }
                if let Some(stderr) = child.stderr.take() {
                    spawn_output_reader(stderr, self.event_tx.clone(), RuntimeLogLevel::Error);
                }
                self.push_log(
                    RuntimeLogLevel::Info,
                    format!("Running {}", executable.display()),
                );
                self.status = RuntimeStatus::Running;
                self.child = Some(child);
            }
            Err(err) => {
                let message = format!("Failed to launch runtime: {}", err);
                self.last_error = Some(message.clone());
                self.push_log(RuntimeLogLevel::Error, message);
                self.status = RuntimeStatus::Failed;
            }
        }
    }

    fn push_log(&mut self, level: RuntimeLogLevel, message: impl Into<String>) {
        self.logs.push(RuntimeLogEntry {
            level,
            message: message.into(),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeStatus {
    Idle,
    Building,
    Running,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct RuntimeLogEntry {
    pub level: RuntimeLogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug)]
enum RuntimeBuildAction {
    BuildOnly,
    BuildAndRun,
    RebuildAll,
}

impl RuntimeBuildAction {
    fn run_after(self) -> bool {
        matches!(self, Self::BuildAndRun)
    }
}

enum RuntimeEvent {
    Log(RuntimeLogEntry),
    Error(String),
    BuildFinished { run: bool },
}

fn ensure_runtime_workspace(
    template_root: &Path,
    runtime_root: &Path,
    action: RuntimeBuildAction,
) -> io::Result<()> {
    fs::create_dir_all(runtime_root)?;

    let build_root = runtime_root.join("build");
    if matches!(action, RuntimeBuildAction::RebuildAll) && build_root.exists() {
        fs::remove_dir_all(build_root)?;
    }

    let cmake_path = runtime_root.join("CMakeLists.txt");
    let main_cpp_path = runtime_root.join("main.cpp");
    if !cmake_path.exists() || !main_cpp_path.exists() {
        for entry in fs::read_dir(template_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let file_name = entry.file_name();
                let destination = runtime_root.join(file_name);
                if !destination.exists() {
                    fs::copy(path, destination)?;
                }
            }
        }
    }
    Ok(())
}

fn run_command(sender: &Sender<RuntimeEvent>, program: &str, args: Vec<String>) -> bool {
    let command_display = format!("{} {}", program, args.join(" "));
    let _ = sender.send(RuntimeEvent::Log(RuntimeLogEntry {
        level: RuntimeLogLevel::Info,
        message: format!("$ {}", command_display),
    }));

    let output = Command::new(program)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) => {
            emit_output(sender, RuntimeLogLevel::Info, output.stdout);
            emit_output(sender, RuntimeLogLevel::Error, output.stderr);
            if output.status.success() {
                true
            } else {
                let _ = sender.send(RuntimeEvent::Error(format!(
                    "Command failed with status {}",
                    output.status
                )));
                false
            }
        }
        Err(err) => {
            let _ = sender.send(RuntimeEvent::Error(format!(
                "Failed to run {}: {}",
                program, err
            )));
            false
        }
    }
}

fn emit_output(sender: &Sender<RuntimeEvent>, level: RuntimeLogLevel, bytes: Vec<u8>) {
    if bytes.is_empty() {
        return;
    }
    let output = String::from_utf8_lossy(&bytes);
    for line in output.lines() {
        let _ = sender.send(RuntimeEvent::Log(RuntimeLogEntry {
            level,
            message: line.to_string(),
        }));
    }
}

fn runtime_executable_path(build_root: &Path) -> PathBuf {
    let exe_name = if cfg!(windows) {
        "meshi_app.exe"
    } else {
        "meshi_app"
    };

    if cfg!(windows) {
        let debug_path = build_root.join("Debug").join(exe_name);
        if debug_path.exists() {
            return debug_path;
        }
    }

    build_root.join(exe_name)
}

fn spawn_output_reader<T: io::Read + Send + 'static>(
    stream: T,
    sender: Sender<RuntimeEvent>,
    level: RuntimeLogLevel,
) {
    thread::spawn(move || {
        let reader = io::BufReader::new(stream);
        for line in reader.lines().flatten() {
            let _ = sender.send(RuntimeEvent::Log(RuntimeLogEntry {
                level,
                message: line,
            }));
        }
    });
}

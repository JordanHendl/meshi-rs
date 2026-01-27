mod app;
mod camera;
mod dbgen;
mod ui;

fn main() {
    tracing_subscriber::fmt::init();
    app::run();
}

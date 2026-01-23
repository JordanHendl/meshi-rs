mod app;
mod dbgen;

fn main() {
    tracing_subscriber::fmt::init();
    app::run();
}

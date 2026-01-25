pub struct RuntimeBridge {
    tick_count: u64,
}

impl RuntimeBridge {
    pub fn new() -> Self {
        Self { tick_count: 0 }
    }

    pub fn tick(&mut self) {
        self.tick_count += 1;
        // TODO: build & launch the C++ runtime using the Meshi wrapper.
    }
}

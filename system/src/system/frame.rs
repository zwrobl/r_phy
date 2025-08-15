pub struct FrameData {
    delta_time: f32,
}

impl FrameData {
    pub fn new(delta_time: f32) -> Self {
        Self { delta_time }
    }

    pub fn set_delta_time(&mut self, delta_time: f32) {
        self.delta_time = delta_time;
    }

    pub fn delta_time(&self) -> f32 {
        self.delta_time
    }
}

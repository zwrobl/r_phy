use math::types::Vector2;

pub struct FrameData {
    delta_time: f32,
    screen_size: Vector2,
}

impl FrameData {
    pub fn new(screen_size: Vector2) -> Self {
        Self {
            delta_time: 0.0,
            screen_size,
        }
    }

    pub fn set_delta_time(&mut self, delta_time: f32) {
        self.delta_time = delta_time;
    }

    pub fn delta_time(&self) -> f32 {
        self.delta_time
    }

    pub fn screen_size(&self) -> Vector2 {
        self.screen_size
    }

    pub fn screen_center(&self) -> Vector2 {
        self.screen_size / 2.0
    }
}

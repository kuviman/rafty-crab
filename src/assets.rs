use super::*;

#[derive(Deserialize)]
pub struct CameraConfig {
    pub distance: f32,
    pub attack: f32,
    pub rot: f32,
    pub fov: f32,
    pub speed: f32,
}

#[derive(Deserialize)]
pub struct CrabAnimation {
    pub legs_freq: f32,
    pub legs_amp: f32,
    pub z: f32,
}

#[derive(Deserialize)]
pub struct WaveConfig {
    pub dir: vec2<f32>,
    pub freq: f32,
    pub vertical_amp: f32,
    pub angle_amp: f32,
    pub speed: f32,
}

#[derive(Deserialize)]
pub struct WaterConfig {
    pub color: Rgba<f32>,
    pub z: f32,
}

#[derive(Deserialize, geng::asset::Load)]
#[load(serde = "toml")]
pub struct Config {
    pub water: WaterConfig,
    pub tile_size: f32,
    pub scaling: f32,
    pub forward_speed: f32,
    pub side_speed: f32,
    pub camera: CameraConfig,
    pub crab_animation: CrabAnimation,
    pub wave: WaveConfig,
}

#[derive(geng::asset::Load)]
pub struct Shaders {
    pub model: ugli::Program,
    pub water: ugli::Program,
}

#[derive(geng::asset::Load)]
pub struct Crab {
    pub body: pog_paint::Model,
    pub legs: pog_paint::Model,
}

#[derive(geng::asset::Load)]
pub struct Assets {
    pub shaders: Shaders,
    pub config: Config,
    pub crab: Crab,
    pub shark: pog_paint::Model,
    pub raft_tile: pog_paint::Model,
}

use super::*;

#[derive(Deserialize)]
pub struct CameraConfig {
    pub distance: f32,
    pub attack: f32,
    pub rot: f32,
    pub fov: f32,
}

#[derive(Deserialize, geng::asset::Load)]
#[load(serde = "toml")]
pub struct Config {
    pub scaling: f32,
    pub forward_speed: f32,
    pub side_speed: f32,
    pub camera: CameraConfig,
}

#[derive(geng::asset::Load)]
pub struct Shaders {
    pub model: ugli::Program,
}

#[derive(geng::asset::Load)]
pub struct Assets {
    pub crab: pog_paint::Model,
    pub shaders: Shaders,
    pub config: Config,
}

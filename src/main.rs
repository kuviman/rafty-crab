use assets::Assets;
use camera::Camera;
use geng::prelude::*;
use model_draw::ModelDraw;

mod assets;
mod camera;
mod model_draw;

#[derive(clap::Parser)]
pub struct Cli {
    #[clap(flatten)]
    pub geng: geng::CliArgs,
}

#[derive(Clone)]
pub struct Ctx {
    geng: Geng,
    assets: Rc<Assets>,
    model_draw: Rc<ModelDraw>,
}

impl Ctx {
    pub async fn new(geng: &Geng) -> Self {
        let assets: Rc<Assets> = geng
            .asset_manager()
            .load(run_dir().join("assets"))
            .await
            .unwrap();
        Self {
            geng: geng.clone(),
            model_draw: Rc::new(ModelDraw::new(geng, &assets)),
            assets,
        }
    }
}

pub struct Pos {
    pub pos: vec3<f32>,
    pub rot: Angle<f32>,
    pub vel: vec3<f32>,
}

impl Pos {
    pub fn transform(&self) -> mat4<f32> {
        mat4::translate(self.pos) * mat4::rotate_z(self.rot)
    }
}

pub struct Game {
    ctx: Ctx,
    pos: Pos,
    camera: Camera,
    framebuffer_size: vec2<f32>,
    time: f32,
    wave_dir: vec2<f32>,
}

impl Game {
    pub fn new(ctx: &Ctx) -> Self {
        Self {
            ctx: ctx.clone(),
            pos: Pos {
                pos: vec3::ZERO,
                rot: Angle::ZERO,
                vel: vec3::ZERO,
            },
            camera: Camera {
                pos: vec3::ZERO,
                fov: Angle::from_degrees(ctx.assets.config.camera.fov),
                rot: Angle::from_degrees(ctx.assets.config.camera.rot),
                attack: Angle::from_degrees(ctx.assets.config.camera.attack),
                distance: ctx.assets.config.camera.distance,
            },
            framebuffer_size: vec2::splat(1.0),
            time: 0.0,
            wave_dir: ctx.assets.config.wave.dir.normalize_or_zero(),
        }
    }
    pub async fn run(mut self) {
        let mut events = self.ctx.geng.window().events();
        let mut timer = Timer::new();
        while let Some(event) = events.next().await {
            match event {
                geng::Event::Draw => {
                    self.update(timer.tick());
                    self.ctx
                        .geng
                        .window()
                        .clone()
                        .with_framebuffer(|framebuffer| self.draw(framebuffer));
                }
                _ => {}
            }
        }
    }

    fn update(&mut self, delta_time: time::Duration) {
        let delta_time = delta_time.as_secs_f64() as f32;
        self.time += delta_time;

        let mut mov = vec2::<f32>::ZERO;
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowLeft)
            || self.ctx.geng.window().is_key_pressed(geng::Key::A)
        {
            mov.x -= 1.0;
        }
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowRight)
            || self.ctx.geng.window().is_key_pressed(geng::Key::D)
        {
            mov.x += 1.0;
        }
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowUp)
            || self.ctx.geng.window().is_key_pressed(geng::Key::W)
        {
            mov.y += 1.0;
        }
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowDown)
            || self.ctx.geng.window().is_key_pressed(geng::Key::S)
        {
            mov.y -= 1.0;
        }
        // relative to crab
        let mov = mov
            .clamp_len(..=1.0)
            .rotate(self.camera.rot)
            .rotate(-self.pos.rot);
        self.pos.vel = (mov
            * vec2(
                self.ctx.assets.config.forward_speed,
                self.ctx.assets.config.side_speed,
            ))
        .rotate(self.pos.rot)
        .extend(0.0);
        self.pos.pos += self.pos.vel * delta_time;

        if let Some(pos) = self.ctx.geng.window().cursor_position() {
            let ray = self
                .camera
                .pixel_ray(self.framebuffer_size, pos.map(|x| x as f32));
            if ray.dir.z < -1e-5 {
                let t = -ray.from.z / ray.dir.z;
                let ground_pos = ray.from + ray.dir * t;
                let delta_pos = ground_pos - self.pos.pos;
                self.pos.rot = delta_pos.xy().arg();
            }
        }

        let delta = self.pos.pos.xy() - self.camera.pos.xy();
        self.camera.pos += (delta * self.ctx.assets.config.camera.speed * delta_time)
            .clamp_len(..=delta.len())
            .extend(0.0);
    }

    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        ugli::clear(
            framebuffer,
            Some(self.ctx.assets.config.water.color),
            Some(1.0),
            None,
        );

        let transform = self.pos.transform()
            * mat4::translate(
                vec3::UNIT_Z
                    * (/*self.height_at(self.pos.pos.xy()) +*/self.ctx.assets.config.crab_animation.z),
            );
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.crab.body,
            transform,
        );
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.crab.legs,
            transform
                * mat4::rotate_z(Angle::from_degrees(
                    (self.time * self.ctx.assets.config.crab_animation.legs_freq).sin()
                        * self.ctx.assets.config.crab_animation.legs_amp
                        * (self.pos.vel.xy().len() / self.ctx.assets.config.side_speed).min(1.0),
                )),
        );

        let angle = Angle::from_degrees(self.time * 10.0);
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.shark,
            mat4::translate(vec2(8.0, 0.0).rotate(angle).extend(-2.0))
                * mat4::rotate_z(angle + Angle::from_degrees(270.0)),
        );

        for x in -1..=1 {
            for y in -1..=1 {
                self.ctx.model_draw.draw(
                    framebuffer,
                    &self.camera,
                    &self.ctx.assets.raft_tile,
                    mat4::translate(
                        (vec2(x, y).map(|x| x as f32) * self.ctx.assets.config.tile_size)
                            .extend(0.0),
                    ) * self.tile_transform(vec2(x, y)),
                );
            }
        }

        // water
        let transform = mat4::translate(self.camera.pos)
            * mat4::translate(vec3(0.0, 0.0, self.ctx.assets.config.water.z))
            * mat4::scale_uniform(1000.0)
            * mat4::translate(vec2::splat(-0.5).extend(0.0));
        ugli::draw(
            framebuffer,
            &self.ctx.assets.shaders.water,
            ugli::DrawMode::TriangleFan,
            &self.ctx.model_draw.quad,
            (
                ugli::uniforms! {
                    u_water_color: self.ctx.assets.config.water.color,
                    u_model_matrix: transform,
                },
                self.camera.uniforms(self.framebuffer_size),
            ),
            ugli::DrawParameters {
                depth_func: Some(ugli::DepthFunc::LessOrEqual),
                blend_mode: Some(ugli::BlendMode::straight_alpha()),
                ..default()
            },
        );
    }

    fn height_at(&self, pos: vec2<f32>) -> f32 {
        let tile = pos.map(|x| (x / self.ctx.assets.config.tile_size).round() as i32);
        let tile_transform = self.tile_transform(tile);
        let pos_in_tile = pos - tile.map(|x| x as f32 * self.ctx.assets.config.tile_size);
        (tile_transform * pos_in_tile.extend(0.0).extend(1.0))
            .into_3d()
            .z
    }

    fn tile_transform(&self, pos: vec2<i32>) -> mat4<f32> {
        let (wave_z, wave_angle) = {
            let (wave_sin, wave_cos) = ((vec2::dot(
                pos.map(|x| x as f32) * self.ctx.assets.config.tile_size,
                self.wave_dir,
            ) + self.time * self.ctx.assets.config.wave.speed)
                * self.ctx.assets.config.wave.freq)
                .sin_cos();
            (
                wave_sin * self.ctx.assets.config.wave.vertical_amp,
                Angle::from_degrees(-wave_cos * self.ctx.assets.config.wave.angle_amp),
            )
        };
        mat4::translate(vec3(0.0, 0.0, wave_z))
            * mat4::rotate(self.wave_dir.rotate_90().extend(0.0), wave_angle)
    }
}

fn main() {
    logger::init();
    geng::setup_panic_handler();
    let cli: Cli = cli::parse();
    Geng::run_with(
        &{
            let mut options = geng::ContextOptions::default();
            options.window.title = "Crabs".to_owned();
            options.with_cli(&cli.geng);
            options
        },
        |geng| async move {
            let ctx = Ctx::new(&geng).await;
            Game::new(&ctx).run().await;
        },
    )
}

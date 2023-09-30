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
                fov: Angle::from_degrees(ctx.assets.config.camera.fov),
                rot: Angle::from_degrees(ctx.assets.config.camera.rot),
                attack: Angle::from_degrees(ctx.assets.config.camera.attack),
                distance: ctx.assets.config.camera.distance,
            },
            framebuffer_size: vec2::splat(1.0),
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
        self.pos.pos += (mov
            * vec2(
                self.ctx.assets.config.forward_speed,
                self.ctx.assets.config.side_speed,
            ))
        .rotate(self.pos.rot)
        .extend(0.0)
            * delta_time;

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
    }

    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        ugli::clear(framebuffer, Some(Rgba::BLACK), Some(1.0), None);
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.crab,
            self.pos.transform(),
        );
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

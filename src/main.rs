use camera::Camera;
use geng::prelude::*;
use model_draw::ModelDraw;

mod camera;
mod model_draw;

#[derive(geng::asset::Load)]
pub struct Shaders {
    pub model: ugli::Program,
}

#[derive(geng::asset::Load)]
pub struct Assets {
    pub crab: pog_paint::Model,
    pub shaders: Shaders,
}

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
                fov: Angle::from_degrees(90.0),
                rot: Angle::from_degrees(30.0),
                attack: Angle::from_degrees(60.0),
                distance: 50.0,
            },
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
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowLeft) {
            self.camera.rot -= Angle::from_degrees(90.0) * delta_time.as_secs_f64() as f32;
        }
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowRight) {
            self.camera.rot += Angle::from_degrees(90.0) * delta_time.as_secs_f64() as f32;
        }
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowUp) {
            self.camera.attack -= Angle::from_degrees(90.0) * delta_time.as_secs_f64() as f32;
        }
        if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowDown) {
            self.camera.attack += Angle::from_degrees(90.0) * delta_time.as_secs_f64() as f32;
        }
    }

    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
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

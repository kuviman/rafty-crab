use assets::Assets;
use camera::Camera;
use geng::prelude::*;
use interpolation::Interpolated;
use model_draw::ModelDraw;

mod assets;
mod camera;
mod model_draw;
#[cfg(not(target_arch = "wasm32"))]
mod server;

mod interpolation;

type Id = i64;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Spawn {
    pub pos: Pos,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMessage {
    YouSpawn(Spawn),
    Pog,
    NewPlayer { id: Id, pos: Pos },
    UpdatePos { id: Id, pos: Pos },
    PlayerLeft { id: Id },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMessage {
    Pig,
    UpdatePos(Pos),
}

#[derive(clap::Parser)]
pub struct Cli {
    #[clap(long)]
    pub server: Option<String>,
    #[clap(long)]
    pub connect: Option<String>,
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

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
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

pub struct InterpolatedPos {
    pub pos: Interpolated<vec3<f32>>,
    pub rot: Interpolated<Angle<f32>>,
}

impl InterpolatedPos {
    pub fn new(pos: Pos) -> Self {
        Self {
            pos: Interpolated::new(pos.pos, pos.vel),
            rot: Interpolated::new(pos.rot, Angle::ZERO),
        }
    }
    pub fn server_update(&mut self, pos: Pos) {
        self.pos.server_update(pos.pos, pos.vel);
        self.rot.server_update(pos.rot, Angle::ZERO);
    }
    pub fn update(&mut self, delta_time: f32) {
        self.pos.update(delta_time);
        self.rot.update(delta_time);
    }
    fn get(&self) -> Pos {
        Pos {
            pos: self.pos.get(),
            rot: self.rot.get(),
            vel: self.pos.get_derivative(),
        }
    }
}

struct OtherPlayer {
    pos: InterpolatedPos,
}

pub struct Game {
    con: geng::net::client::Connection<ServerMessage, ClientMessage>,
    ctx: Ctx,
    pos: Pos,
    camera: Camera,
    framebuffer_size: vec2<f32>,
    time: f32,
    wave_dir: vec2<f32>,
    others: HashMap<Id, OtherPlayer>,
}

impl Game {
    pub fn new(
        ctx: &Ctx,
        con: geng::net::client::Connection<ServerMessage, ClientMessage>,
        spawn: Spawn,
    ) -> Self {
        Self {
            con,
            ctx: ctx.clone(),
            pos: spawn.pos,
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
            others: default(),
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

            let new_messages: Vec<_> = self.con.new_messages().collect();
            for message in new_messages {
                self.handle_server(message.unwrap());
            }
        }
    }

    fn handle_server(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::YouSpawn(spawn) => {
                self.pos = spawn.pos;
            }
            ServerMessage::NewPlayer { id, pos } => {
                self.others.insert(
                    id,
                    OtherPlayer {
                        pos: InterpolatedPos::new(pos),
                    },
                );
            }
            ServerMessage::UpdatePos { id, pos } => {
                self.others.get_mut(&id).unwrap().pos.server_update(pos);
            }
            ServerMessage::PlayerLeft { id } => {
                self.others.remove(&id);
            }
            ServerMessage::Pog => {
                self.con.send(ClientMessage::Pig);
                self.con.send(ClientMessage::UpdatePos(self.pos));
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

        for other in self.others.values_mut() {
            other.pos.update(delta_time);
        }
    }

    fn draw_crab(&self, framebuffer: &mut ugli::Framebuffer, pos: Pos) {
        let transform = pos.transform()
            * mat4::translate(
                vec3::UNIT_Z
                    * (/*self.height_at(pos.pos.xy()) +*/self.ctx.assets.config.crab_animation.z),
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
                        * (pos.vel.xy().len() / self.ctx.assets.config.side_speed).min(1.0),
                )),
        );
    }

    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        ugli::clear(
            framebuffer,
            Some(self.ctx.assets.config.water.color),
            Some(1.0),
            None,
        );

        self.draw_crab(framebuffer, self.pos);
        for other in self.others.values() {
            self.draw_crab(framebuffer, other.pos.get());
        }

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
    let mut cli: Cli = cli::parse();

    if cli.connect.is_none() && cli.server.is_none() {
        #[cfg(target_arch = "wasm32")]
        {
            cli.connect = Some(
                option_env!("CONNECT")
                    .filter(|addr| !addr.is_empty())
                    .map(|addr| addr.to_owned())
                    .unwrap_or_else(|| {
                        let window = web_sys::window().unwrap();
                        let location = window.location();
                        let mut new_uri = String::new();
                        if location.protocol().unwrap() == "https" {
                            new_uri += "wss://";
                        } else {
                            new_uri += "ws://";
                        }
                        new_uri += &location.host().unwrap();
                        new_uri += &location.pathname().unwrap();
                        new_uri
                    }),
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            cli.server = Some("127.0.0.1:1155".to_owned());
            cli.connect = Some("ws://127.0.0.1:1155".to_owned());
        }
    }
    if cli.server.is_some() && cli.connect.is_none() {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let server = geng::net::Server::new(server::App::new(), cli.server.as_deref().unwrap());
            let server_handle = server.handle();
            ctrlc::set_handler(move || server_handle.shutdown()).unwrap();
            server.run();
        }
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        let server = if let Some(addr) = &cli.server {
            let server = geng::net::Server::new(server::App::new(), addr);
            let server_handle = server.handle();
            let server_thread = std::thread::spawn(move || {
                server.run();
            });
            Some((server_handle, server_thread))
        } else {
            None
        };

        Geng::run_with(
            &{
                let mut options = geng::ContextOptions::default();
                options.window.title = "Crabs".to_owned();
                options.with_cli(&cli.geng);
                options
            },
            |geng| async move {
                let ctx = Ctx::new(&geng).await;
                let mut con = geng::net::client::connect(cli.connect.as_deref().unwrap())
                    .await
                    .unwrap();
                let ServerMessage::YouSpawn(spawn) = con.next().await.unwrap().unwrap() else {
                    panic!()
                };
                Game::new(&ctx, con, spawn).run().await;
            },
        );

        #[cfg(not(target_arch = "wasm32"))]
        if let Some((server_handle, server_thread)) = server {
            server_handle.shutdown();
            server_thread.join().unwrap();
        }
    }
}

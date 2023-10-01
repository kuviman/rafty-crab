use assets::Assets;
use camera::Camera;
use geng::prelude::{bincode::de, *};
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
pub struct Shark {
    pub destroy_timer: Option<f32>,
    pub pos: Pos,
    pub destroy: Option<vec2<i32>>,
    pub target_pos: vec2<f32>,
}

struct InterpolatedShark {
    pos: InterpolatedPos,
}

impl InterpolatedShark {
    pub fn new(shark: Shark) -> Self {
        Self {
            pos: InterpolatedPos::new(shark.pos),
        }
    }
    pub fn server_update(&mut self, upd: Shark) {
        self.pos.server_update(upd.pos);
    }
    fn update(&mut self, delta_time: f32) {
        self.pos.update(delta_time);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMessage {
    UpdateRaft(HashSet<vec2<i32>>),
    YouSpawn(Spawn),
    YouDrown,
    Pog,
    PlayerSpawn { id: Id, pos: Pos },
    UpdatePos { id: Id, pos: Pos },
    PlayerLeft { id: Id },
    UpdateSharks(HashMap<i64, Shark>),
    PlayerDrown(i64),
    Destroy(Id, vec2<i32>),
    AboutToDestroy(i64, vec2<i32>),
    JustRestarted,
    YouDash(vec3<f32>),
    DashRestore,
    YouStartAttack(vec2<f32>),
    StartAttack(vec2<f32>, i64),
    Dash(i64, Pos),
    YouWasPushed(vec2<f32>),
    WasPushed(i64, Pos),
    Name(i64, String),
    Damage(vec3<f32>),
    UpdateGullPos { id: i64, pos: Pos },
    YouCanPoopCongratulations,
    FlyingPoop(Pos),
    PoopOnFloor(vec2<f32>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMessage {
    Pig,
    UpdatePos(Pos),
    Attack(vec3<f32>),
    TeleportAck,
    Name(String),
    UpdateGullPos(Pos),
    Poop,
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

struct Vfx {
    model: Rc<pog_paint::Model>,
    t: f32,
    max_t: f32,
    pos: vec3<f32>,
    rot: Angle<f32>,
}
impl Vfx {
    fn new(model: &Rc<pog_paint::Model>, pos: vec3<f32>) -> Vfx {
        Self::new_rot(model, pos, thread_rng().gen())
    }
    fn new_rot(model: &Rc<pog_paint::Model>, pos: vec3<f32>, rot: Angle<f32>) -> Vfx {
        Self {
            model: model.clone(),
            t: 0.0,
            max_t: 0.3,
            pos,
            rot,
        }
    }
}

pub struct Game {
    can_poop: bool,
    floor_poop: Vec<vec2<f32>>,
    flying_poops: Vec<Pos>,
    names: HashMap<Id, String>,
    name: String,
    naming: bool,
    attacks: HashSet<Id>,
    attacking: bool,
    can_dash: bool,
    shark_attacks: HashMap<Id, vec2<i32>>,
    con: geng::net::client::Connection<ServerMessage, ClientMessage>,
    ctx: Ctx,
    me: Option<Pos>,
    me_gull: Pos,
    camera: Camera,
    framebuffer_size: vec2<f32>,
    time: f32,
    wave_dir: vec2<f32>,
    others: HashMap<Id, OtherPlayer>,
    other_gulls: HashMap<Id, OtherPlayer>,
    raft: HashSet<vec2<i32>>,
    sharks: HashMap<Id, InterpolatedShark>,
    vfx: Vec<Vfx>,
}

impl Game {
    pub fn new(
        ctx: &Ctx,
        con: geng::net::client::Connection<ServerMessage, ClientMessage>,
    ) -> Self {
        Self {
            floor_poop: default(),
            flying_poops: default(),
            can_poop: true,
            other_gulls: default(),
            names: default(),
            name: "".to_owned(),
            naming: true,
            me_gull: Pos {
                pos: thread_rng()
                    .gen_circle(
                        vec2::ZERO,
                        ctx.assets.config.raft_size as f32 * ctx.assets.config.tile_size,
                    )
                    .extend(ctx.assets.config.seagull_height),
                rot: thread_rng().gen(),
                vel: vec2(ctx.assets.config.seagull_speed, 0.0)
                    .rotate(thread_rng().gen())
                    .extend(0.0),
            },
            attacks: default(),
            attacking: false,
            can_dash: true,
            shark_attacks: default(),
            con,
            ctx: ctx.clone(),
            me: None,
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
            raft: default(),
            sharks: default(),
            vfx: default(),
        }
    }
    pub async fn run(mut self) {
        let mut events = self.ctx.geng.window().events();
        let mut timer = Timer::new();
        while let Some(event) = events.next().await {
            match event {
                geng::Event::EditText(new_name) => {
                    self.name = new_name;
                }
                geng::Event::KeyPress { key } if self.naming => {
                    if key == geng::Key::Enter {
                        self.naming = false;
                        self.con.send(ClientMessage::Name(self.name.clone()));
                    }
                    let t = format!("{key:?}");
                    if t.len() == 1 && self.name.len() < 15 {
                        self.name.push_str(&t);
                    }
                    if key == geng::Key::Backspace {
                        self.name.pop();
                    }
                }
                geng::Event::KeyPress {
                    key: geng::Key::Space,
                }
                | geng::Event::MousePress {
                    button: geng::MouseButton::Left,
                } if self.me.is_none() && self.can_poop => {
                    self.con.send(ClientMessage::Poop);
                }
                geng::Event::MousePress {
                    button: geng::MouseButton::Left,
                } if self.can_dash => {
                    if let Some(pos) = self.ctx.geng.window().cursor_position() {
                        let ray = self
                            .camera
                            .pixel_ray(self.framebuffer_size, pos.map(|x| x as f32));
                        if ray.dir.z < -1e-5 {
                            let t = -ray.from.z / ray.dir.z;
                            let ground_pos = ray.from + ray.dir * t;

                            if let Some(me) = &self.me {
                                self.con.send(ClientMessage::Attack(ground_pos));
                                self.attacking = true;
                                self.can_dash = false;
                            }
                        }
                    }
                }
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
            ServerMessage::PoopOnFloor(pos) => {
                self.floor_poop.push(pos);
                self.ctx.assets.sfx.wet_fart.play();
            }
            ServerMessage::FlyingPoop(pos) => {
                self.flying_poops.push(pos);
                self.ctx.assets.sfx.dry_fart.play();
            }
            ServerMessage::YouCanPoopCongratulations => {
                self.can_poop = true;
            }
            ServerMessage::UpdateGullPos { id, pos } => match self.other_gulls.entry(id) {
                std::collections::hash_map::Entry::Occupied(mut other) => {
                    other.get_mut().pos.server_update(pos);
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(OtherPlayer {
                        pos: InterpolatedPos::new(pos),
                    });
                }
            },
            ServerMessage::Damage(pos) => {
                self.vfx
                    .push(Vfx::new(&self.ctx.assets.damage, pos + vec3(0.0, 0.0, 1.2)));
            }
            ServerMessage::Name(id, name) => {
                self.names.insert(id, name);
            }
            ServerMessage::WasPushed(id, new_pos) => {
                self.ctx.assets.sfx.bonk.play();
                if let Some(other) = self.others.get_mut(&id) {
                    let old_pos = other.pos.get().pos;
                    other.pos.server_update(new_pos);
                    other.pos.update(1.0);

                    self.vfx.push(Vfx::new_rot(
                        &self.ctx.assets.push,
                        new_pos.pos,
                        (new_pos.pos - old_pos).xy().arg(),
                    ));
                }
            }
            ServerMessage::YouWasPushed(delta) => {
                self.con.send(ClientMessage::TeleportAck);
                if let Some(me) = &mut self.me {
                    me.pos += delta.extend(0.0);
                    self.ctx.assets.sfx.bonk.play();

                    self.vfx
                        .push(Vfx::new_rot(&self.ctx.assets.push, me.pos, delta.arg()));
                }
            }
            ServerMessage::StartAttack(_new_pos, id) => {
                self.attacks.insert(id);
            }
            ServerMessage::Dash(id, new_pos) => {
                self.attacks.remove(&id);
                if let Some(other) = self.others.get_mut(&id) {
                    let old_pos = other.pos.get().pos;
                    other.pos.server_update(new_pos);
                    other.pos.update(1.0);
                    let new_pos = new_pos.pos;
                    self.ctx.assets.sfx.dash.play();
                    self.vfx.push(Vfx::new_rot(
                        &self.ctx.assets.dash,
                        new_pos,
                        (new_pos - old_pos).xy().arg(),
                    ));
                }
            }
            ServerMessage::YouStartAttack(_) => {}
            ServerMessage::DashRestore => {
                self.can_dash = true;
            }
            ServerMessage::YouDash(new_pos) => {
                self.con.send(ClientMessage::TeleportAck);
                if let Some(me) = &mut self.me {
                    let old_pos = me.pos;
                    me.pos = new_pos;
                    self.attacking = false;
                    self.ctx.assets.sfx.dash.play();
                    self.vfx.push(Vfx::new_rot(
                        &self.ctx.assets.dash,
                        new_pos,
                        (new_pos - old_pos).xy().arg(),
                    ));
                }
            }
            ServerMessage::JustRestarted => {
                self.shark_attacks.clear();
                self.attacks.clear();
                self.floor_poop.clear();
            }
            ServerMessage::Destroy(shark, tile) => {
                self.shark_attacks.remove(&shark);
                self.raft.remove(&tile);
                self.ctx.assets.sfx.destroy.play();
                self.vfx.push(Vfx::new(
                    &self.ctx.assets.destroy,
                    tile.map(|x| x as f32 * self.ctx.assets.config.tile_size)
                        .extend(0.0),
                ));
            }
            ServerMessage::AboutToDestroy(shark, tile) => {
                self.shark_attacks.insert(shark, tile);
                self.ctx.assets.sfx.eating.play();
            }
            ServerMessage::PlayerDrown(id) => {
                if let Some(other) = self.others.remove(&id) {
                    self.vfx.push(Vfx::new(
                        &self.ctx.assets.splash,
                        other.pos.get().pos.xy().extend(0.0),
                    ));
                    self.ctx.assets.sfx.splash.play();
                }
            }
            ServerMessage::YouDrown => {
                if let Some(me) = self.me.take() {
                    self.vfx
                        .push(Vfx::new(&self.ctx.assets.splash, me.pos.xy().extend(0.0)));
                    self.ctx.assets.sfx.splash.play();
                }
            }
            ServerMessage::YouSpawn(spawn) => {
                self.me = Some(spawn.pos);
                self.attacking = false;
                self.can_dash = true;
            }
            ServerMessage::PlayerSpawn { id, pos } => {
                self.others.insert(
                    id,
                    OtherPlayer {
                        pos: InterpolatedPos::new(pos),
                    },
                );
                self.attacks.remove(&id);
            }
            ServerMessage::UpdatePos { id, pos } => {
                self.others.get_mut(&id).unwrap().pos.server_update(pos);
            }
            ServerMessage::PlayerLeft { id } => {
                self.others.remove(&id);
                self.other_gulls.remove(&id);
            }
            ServerMessage::Pog => {
                self.con.send(ClientMessage::Pig);
                if let Some(me) = &self.me {
                    self.con.send(ClientMessage::UpdatePos(me.clone()));
                } else {
                    self.con.send(ClientMessage::UpdateGullPos(self.me_gull));
                }
            }
            ServerMessage::UpdateRaft(raft) => {
                self.raft = raft;
            }
            ServerMessage::UpdateSharks(sharks) => {
                self.sharks.retain(|id, _| sharks.contains_key(id));
                for (id, shark) in sharks {
                    if let Some(cur) = self.sharks.get_mut(&id) {
                        cur.server_update(shark);
                    } else {
                        self.sharks.insert(id, InterpolatedShark::new(shark));
                    }
                }
            }
        }
    }

    fn update(&mut self, delta_time: time::Duration) {
        let delta_time = delta_time.as_secs_f64() as f32;
        self.time += delta_time;

        if let Some(me) = &mut self.me {
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
                .rotate(-me.rot);

            let on_poop = self
                .floor_poop
                .iter()
                .copied()
                .any(|poop| (poop - me.pos.xy()).len() < 3.0);
            if on_poop {
            } else if self.attacking {
                me.vel = vec3::ZERO;
            } else {
                me.vel = (mov
                    * vec2(
                        self.ctx.assets.config.forward_speed,
                        self.ctx.assets.config.side_speed,
                    ))
                .rotate(me.rot)
                .extend(0.0);
            }
            me.pos += me.vel * delta_time;

            for other in self.others.values() {
                let delta = me.pos.xy() - other.pos.get().pos.xy();
                if delta.len() < 2.0 {
                    let pen = 2.0 - delta.len();
                    me.pos += (delta.normalize_or_zero() * pen)
                        .clamp_len(..=self.ctx.assets.config.collide_speed * delta_time)
                        .extend(0.0);
                }
            }

            if let Some(pos) = self.ctx.geng.window().cursor_position() {
                let ray = self
                    .camera
                    .pixel_ray(self.framebuffer_size, pos.map(|x| x as f32));
                if ray.dir.z < -1e-5 {
                    let t = -ray.from.z / ray.dir.z;
                    let ground_pos = ray.from + ray.dir * t;
                    let delta_pos = ground_pos - me.pos;
                    if !self.attacking {
                        me.rot = delta_pos.xy().arg();
                    }
                }
            }
        } else {
            if self.me_gull.pos.xy().len() > self.ctx.assets.config.limit {
                self.me_gull.rot -=
                    Angle::from_degrees(self.ctx.assets.config.seagull_rotate_speed)
                        * delta_time
                        * vec2::skew(self.me_gull.vel.xy(), self.me_gull.pos.xy()).signum();
            } else {
                if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowLeft)
                    || self.ctx.geng.window().is_key_pressed(geng::Key::A)
                {
                    self.me_gull.rot +=
                        Angle::from_degrees(self.ctx.assets.config.seagull_rotate_speed)
                            * delta_time;
                }

                if self.ctx.geng.window().is_key_pressed(geng::Key::ArrowRight)
                    || self.ctx.geng.window().is_key_pressed(geng::Key::D)
                {
                    self.me_gull.rot -=
                        Angle::from_degrees(self.ctx.assets.config.seagull_rotate_speed)
                            * delta_time;
                }
            }
            self.me_gull.vel = vec2(self.ctx.assets.config.seagull_speed, 0.0)
                .rotate(self.me_gull.rot)
                .extend(0.0);
            self.me_gull.pos += self.me_gull.vel * delta_time;
        }

        let target_pos = self.me.unwrap_or(self.me_gull).pos;
        let delta = target_pos - self.camera.pos;
        self.camera.pos +=
            (delta * self.ctx.assets.config.camera.speed * delta_time).clamp_len(..=delta.len());

        for other in self.others.values_mut() {
            other.pos.update(delta_time);
        }
        for shark in self.sharks.values_mut() {
            shark.update(delta_time);
        }
        for other in self.other_gulls.values_mut() {
            other.pos.update(delta_time);
        }

        for vfx in &mut self.vfx {
            vfx.t += delta_time;
        }
        self.vfx.retain(|vfx| vfx.t < vfx.max_t);

        for poop in &mut self.flying_poops {
            poop.vel.z -= self.ctx.assets.config.gravity * delta_time;
            poop.pos += poop.vel * delta_time;
        }
        self.flying_poops.retain(|poop| poop.pos.z > 0.0);
    }

    fn draw_crab(&self, framebuffer: &mut ugli::Framebuffer, pos: Pos, attacking: bool) {
        let winner = self.others.len() + self.me.is_some() as usize == 1;
        let on_poop = self
            .floor_poop
            .iter()
            .copied()
            .any(|poop| (poop - pos.pos.xy()).len() < 3.0);
        let mut transform = pos.transform()
            * mat4::translate(
                vec3::UNIT_Z
                    * (/*self.height_at(pos.pos.xy()) +*/self.ctx.assets.config.crab_animation.z),
            );
        if attacking {
            transform *= mat4::rotate_y(-Angle::from_degrees(40.0));
        } else if winner {
            transform *= mat4::translate(vec3(0.0, 0.0, (self.time * 10.0).sin().abs() * 0.5));
        }
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.crab.body,
            transform,
        );
        if !on_poop {
            transform *= mat4::rotate_z(Angle::from_degrees(
                (self.time * self.ctx.assets.config.crab_animation.legs_freq).sin()
                    * self.ctx.assets.config.crab_animation.legs_amp
                    * (pos.vel.xy().len() / self.ctx.assets.config.side_speed).min(1.0),
            ));
        }
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.crab.legs,
            transform,
        );
    }

    fn draw_gull(&self, framebuffer: &mut ugli::Framebuffer, pos: Pos) {
        let transform = pos.transform();
        self.ctx.model_draw.draw(
            framebuffer,
            &self.camera,
            &self.ctx.assets.seagull,
            transform,
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

        if let Some(me) = &self.me {
            self.draw_crab(framebuffer, *me, self.attacking);
        } else {
            self.draw_gull(framebuffer, self.me_gull);
        }
        for (&id, other) in &self.others {
            self.draw_crab(framebuffer, other.pos.get(), self.attacks.contains(&id));
        }
        for (&id, other) in &self.other_gulls {
            if !self.others.contains_key(&id) {
                self.draw_gull(framebuffer, other.pos.get());
            }
        }
        for poop in &self.flying_poops {
            self.ctx.model_draw.draw(
                framebuffer,
                &self.camera,
                &self.ctx.assets.falling_poop,
                poop.transform(),
            );
        }

        for (id, shark) in &self.sharks {
            if let Some(tile) = self.shark_attacks.get(id) {
                let pos = shark.pos.get().pos;
                self.ctx.model_draw.draw(
                    framebuffer,
                    &self.camera,
                    &self.ctx.assets.shark,
                    mat4::translate(pos + vec3(0.0, 0.0, 2.5))
                        * mat4::rotate_z(
                            (tile.map(|x| x as f32 * self.ctx.assets.config.tile_size) - pos.xy())
                                .arg(),
                        )
                        * mat4::rotate_y(Angle::from_degrees(-50.0))
                        * mat4::rotate_z(Angle::from_degrees(180.0)),
                );
            } else {
                self.ctx.model_draw.draw(
                    framebuffer,
                    &self.camera,
                    &self.ctx.assets.shark,
                    shark.pos.get().transform() * mat4::rotate_z(Angle::from_degrees(180.0)),
                );
            }
        }

        for &tile in &self.raft {
            self.ctx.model_draw.draw(
                framebuffer,
                &self.camera,
                &self.ctx.assets.raft_tile,
                mat4::translate(
                    (tile.map(|x| x as f32) * self.ctx.assets.config.tile_size).extend(0.0),
                ) * self.tile_transform(tile),
            );
        }

        if let Some(bb) = Aabb2::points_bounding_box(self.raft.iter().copied()) {
            let bb = bb.extend_uniform(1);
            let mut raft_texture = ugli::Texture::new_with(
                self.ctx.geng.ugli(),
                bb.size().map(|x| x as usize + 1),
                |pos| {
                    if self
                        .raft
                        .contains(&(pos.map(|x| x as i32) + bb.bottom_left()))
                    {
                        Rgba::WHITE
                    } else {
                        Rgba::TRANSPARENT_BLACK
                    }
                },
            );
            raft_texture.set_filter(ugli::Filter::Nearest);
            for poop in &self.floor_poop {
                self.ctx.model_draw.draw_masked(
                    framebuffer,
                    &self.camera,
                    &self.ctx.assets.poop,
                    mat4::translate(poop.extend(0.5)),
                    &raft_texture,
                    mat3::scale(raft_texture.size().map(|x| 1.0 / x as f32))
                        * mat3::translate(-bb.bottom_left().map(|x| x as f32 - 0.5))
                        * mat3::scale_uniform(1.0 / self.ctx.assets.config.tile_size),
                );
            }
        }

        // vfx
        for vfx in &self.vfx {
            self.ctx.model_draw.draw(
                framebuffer,
                &self.camera,
                &vfx.model,
                mat4::translate(vfx.pos)
                    * mat4::rotate_z(vfx.rot)
                    * mat4::scale_uniform(1.0 + (vfx.t / vfx.max_t) * 0.5),
            );
        }

        // water
        let transform = mat4::translate(self.camera.pos.xy().extend(0.0))
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

        if self.naming {
            let font = self.ctx.geng.default_font();
            let camera = geng::Camera2d {
                fov: 10.0,
                center: vec2::ZERO,
                rotation: Angle::ZERO,
            };
            font.draw(
                framebuffer,
                &camera,
                "Type your name:",
                vec2::splat(geng::TextAlign::CENTER),
                mat3::translate(vec2(0.0, 2.0)),
                Rgba::new(0.2, 0.2, 0.2, 1.0),
            );
            font.draw(
                framebuffer,
                &camera,
                &self.name,
                vec2::splat(geng::TextAlign::CENTER),
                mat3::identity(),
                Rgba::BLACK,
            );
        } else {
            self.draw_name(framebuffer, &self.name, self.me.unwrap_or(self.me_gull));
            for (&id, name) in &self.names {
                if let Some(other) = self.others.get(&id).or(self.other_gulls.get(&id)) {
                    self.draw_name(framebuffer, name, other.pos.get());
                }
            }
        }
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

    fn draw_name(&self, framebuffer: &mut ugli::Framebuffer<'_>, name: &str, pos: Pos) {
        let font = self.ctx.geng.default_font();
        let Some(texture) = font.create_text_sdf(name, geng::TextAlign::CENTER, 32.0) else {
            return;
        };

        let transform = mat4::translate(pos.pos + vec3(0.0, 0.0, 1.5))
            * mat4::rotate_z(-self.camera.rot)
            * mat4::rotate_x(-Angle::from_degrees(270.0) - self.camera.attack)
            * mat4::scale(vec3(texture.size().map(|x| x as f32).aspect(), 1.0, 1.0))
            * mat4::translate(vec3(-0.5, 0.0, 0.0));
        ugli::draw(
            framebuffer,
            &self.ctx.assets.shaders.text,
            ugli::DrawMode::TriangleFan,
            &self.ctx.model_draw.quad,
            (
                ugli::uniforms! {
                    u_texture: &texture,
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
                let con = geng::net::client::connect(cli.connect.as_deref().unwrap())
                    .await
                    .unwrap();
                Game::new(&ctx, con).run().await;
            },
        );

        #[cfg(not(target_arch = "wasm32"))]
        if let Some((server_handle, server_thread)) = server {
            server_handle.shutdown();
            server_thread.join().unwrap();
        }
    }
}

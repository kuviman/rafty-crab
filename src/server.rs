use geng::net::Server;

use super::*;

struct State {
    names: HashMap<Id, String>,
    attacks: HashMap<Id, (vec2<f32>, f32)>,
    should_exit: bool,
    config: assets::Config,
    id_gen: IdGen,
    player_pos: HashMap<Id, Pos>,
    gull_pos: HashMap<Id, Pos>,
    raft: HashSet<vec2<i32>>,
    senders: HashMap<Id, Box<dyn geng::net::Sender<ServerMessage>>>,
    sharks: HashMap<Id, Shark>,
    restart_timer: Option<f32>,
    dash_cooldowns: HashMap<Id, f32>,
    poop_cooldowns: HashMap<Id, f32>,
    wait_for_teleport_ack: HashSet<Id>,
    flying_poops: Vec<Pos>,
}

fn intersect(from: vec2<f32>, dir: vec2<f32>, center: vec2<f32>, radius: f32) -> Option<f32> {
    if (from - center).len() < radius {
        return Some(0.0);
    }
    let d = vec2::skew(center - from, dir).abs();
    if d >= radius {
        return None;
    }
    let x = (radius.sqr() - d.sqr()).sqrt();

    // dot(from + dir * t - center, dir) = 0
    let t = vec2::dot(center - from, dir) / vec2::dot(dir, dir);
    let t = t - x;
    (t > 0.0).then_some(t)
}

struct IdGen {
    last_id: Id,
}

impl IdGen {
    fn gen(&mut self) -> Id {
        self.last_id += 1;
        self.last_id
    }
}

impl State {
    fn restart(&mut self) {
        for shark in self.sharks.values_mut() {
            shark.destroy = None;
            shark.destroy_timer = None;
        }
        self.raft = Aabb2::ZERO
            .extend_uniform(self.config.raft_size)
            .extend_positive(vec2::splat(1))
            .points()
            .filter(|tile| tile.map(|x| x as f32).len() <= self.config.raft_size as f32 + 0.5)
            .collect();
        for (&client, sender) in &mut self.senders {
            sender.send(ServerMessage::UpdateRaft(self.raft.clone()));

            if self.names.contains_key(&client) {
                let pos = Pos {
                    pos: vec3(
                        thread_rng().gen_range(-1.0..=1.0),
                        thread_rng().gen_range(-1.0..=1.0),
                        0.0,
                    ),
                    rot: Angle::from_degrees(thread_rng().gen_range(0.0..360.0)),
                    vel: vec3::ZERO,
                };
                self.player_pos.insert(client, pos);
                sender.send(ServerMessage::YouSpawn(Spawn { pos }));
            }
            sender.send(ServerMessage::JustRestarted);
        }

        for (&id, &pos) in &self.player_pos {
            for (&client, sender) in &mut self.senders {
                if client != id {
                    sender.send(ServerMessage::PlayerSpawn { id, pos });
                }
            }
        }
    }
    fn new(config: assets::Config) -> Self {
        let mut id_gen = IdGen { last_id: 0 };
        Self {
            flying_poops: Vec::new(),
            poop_cooldowns: default(),
            names: default(),
            wait_for_teleport_ack: default(),
            attacks: default(),
            dash_cooldowns: default(),
            restart_timer: None,
            player_pos: default(),
            senders: default(),
            raft: default(),
            should_exit: false,
            gull_pos: default(),
            sharks: (0..config.shark.count)
                .map(|_| {
                    let pos = thread_rng()
                        .gen_circle(vec2::ZERO, config.raft_size as f32 * config.tile_size);
                    (
                        id_gen.gen(),
                        Shark {
                            destroy: None,
                            destroy_timer: None,
                            pos: Pos {
                                pos: pos.extend(config.shark.depth),
                                rot: thread_rng().gen(),
                                vel: vec3::ZERO,
                            },
                            target_pos: pos,
                        },
                    )
                })
                .collect(),
            id_gen,
            config,
        }
    }
    pub fn new_player(&mut self, mut sender: Box<dyn geng::net::Sender<ServerMessage>>) -> Id {
        let id = self.id_gen.gen();
        sender.send(ServerMessage::Pog);
        for (&other_id, &pos) in &self.player_pos {
            if other_id != id {
                sender.send(ServerMessage::PlayerSpawn { id: other_id, pos });
            }
        }
        sender.send(ServerMessage::UpdateRaft(self.raft.clone()));
        sender.send(ServerMessage::UpdateSharks(self.sharks.clone()));
        for (&other_id, name) in &self.names {
            sender.send(ServerMessage::Name(other_id, name.clone()));
        }
        self.senders.insert(id, sender);
        id
    }
    pub fn drop_player(&mut self, client: Id) {
        self.player_pos.remove(&client);
        self.senders.remove(&client);
        for sender in self.senders.values_mut() {
            sender.send(ServerMessage::PlayerLeft { id: client });
        }
        self.gull_pos.remove(&client);
        self.names.remove(&client);
    }
    pub fn handle(&mut self, client: Id, message: ClientMessage) {
        let sender = self.senders.get_mut(&client).unwrap();
        match message {
            ClientMessage::Poop => {
                if self.poop_cooldowns.contains_key(&client) {
                    return;
                }
                if let Some(&pos) = self.gull_pos.get(&client) {
                    self.poop_cooldowns
                        .insert(client, self.config.poop_cooldown);
                    self.flying_poops.push(pos);
                    for client in self.senders.values_mut() {
                        client.send(ServerMessage::FlyingPoop(pos))
                    }
                }
            }
            ClientMessage::Name(name) => {
                for (&id, other) in &mut self.senders {
                    if id != client {
                        other.send(ServerMessage::Name(client, name.clone()));
                    }
                }
                self.names.insert(client, name);
            }
            ClientMessage::Attack(target) => {
                if !self.dash_cooldowns.contains_key(&client) {
                    if let Some(pos) = &self.player_pos.get(&client) {
                        let dir = (target - pos.pos).xy().normalize_or_zero();
                        self.attacks.insert(client, (dir, self.config.attack_time));
                        sender.send(ServerMessage::YouStartAttack(dir));
                        for (&id, other) in &mut self.senders {
                            if id != client {
                                other.send(ServerMessage::StartAttack(dir, client));
                            }
                        }
                    }
                }
            }
            ClientMessage::TeleportAck => {
                self.wait_for_teleport_ack.remove(&client);
            }
            ClientMessage::UpdateGullPos(pos) => {
                self.gull_pos.insert(client, pos);
            }
            ClientMessage::UpdatePos(pos) => {
                if self.wait_for_teleport_ack.contains(&client) {
                    return;
                }
                if let std::collections::hash_map::Entry::Occupied(mut e) =
                    self.player_pos.entry(client)
                {
                    e.insert(pos);
                }
            }
            ClientMessage::Pig => {
                sender.send(ServerMessage::Pog);
                for (&id, &pos) in &self.player_pos {
                    if id != client {
                        sender.send(ServerMessage::UpdatePos { id, pos });
                    }
                }
                for (&id, &pos) in &self.gull_pos {
                    if id != client {
                        sender.send(ServerMessage::UpdateGullPos { id, pos });
                    }
                }
                sender.send(ServerMessage::UpdateSharks(self.sharks.clone()));
            }
        }
    }
    fn tick(&mut self, delta_time: f32) {
        if self.senders.is_empty() {
            return;
        }

        for poop in &mut self.flying_poops {
            poop.vel.z -= self.config.gravity * delta_time;
            poop.pos += poop.vel * delta_time;
            if poop.pos.z <= 0.0 {
                for sender in self.senders.values_mut() {
                    sender.send(ServerMessage::PoopOnFloor(poop.pos.xy()));
                }
            }
        }
        self.flying_poops.retain(|poop| poop.pos.z > 0.0);

        for (client, pos) in self.player_pos.clone() {
            if Aabb2::ZERO
                .extend_uniform(1)
                .extend_positive(vec2::splat(1))
                .points()
                .all(|p| {
                    let tile = (pos.pos.xy() + p.map(|x| x as f32))
                        .map(|x| (x / self.config.tile_size).round() as i32);
                    !self.raft.contains(&tile)
                })
            {
                self.player_pos.remove(&client);
                if let Some(sender) = self.senders.get_mut(&client) {
                    sender.send(ServerMessage::YouDrown);
                }
                for (&id, other) in &mut self.senders {
                    if id == client {
                        continue;
                    }
                    other.send(ServerMessage::PlayerDrown(client));
                }
            }
        }

        for time in self.dash_cooldowns.values_mut() {
            *time -= delta_time;
        }
        self.dash_cooldowns.retain(|&client, &mut time| {
            if time > 0.0 {
                true
            } else {
                if let Some(sender) = self.senders.get_mut(&client) {
                    sender.send(ServerMessage::DashRestore);
                }
                false
            }
        });
        for time in self.poop_cooldowns.values_mut() {
            *time -= delta_time;
        }
        self.poop_cooldowns.retain(|&client, &mut time| {
            if time > 0.0 {
                true
            } else {
                if let Some(sender) = self.senders.get_mut(&client) {
                    sender.send(ServerMessage::YouCanPoopCongratualtions);
                }
                false
            }
        });

        for (_, time) in self.attacks.values_mut() {
            *time -= delta_time;
        }
        self.attacks.retain(|&client, &mut (dir, time)| {
            if time > 0.0 {
                true
            } else {
                if let Some(pos) = self.player_pos.get(&client).copied() {
                    let mut dist = self.config.dash_distance;
                    if let Some((id, t)) = self
                        .player_pos
                        .iter()
                        .filter(|(id, _)| **id != client)
                        .filter_map(|(&id, other)| {
                            intersect(pos.pos.xy(), dir, other.pos.xy(), 2.0).map(|t| (id, t))
                        })
                        .min_by_key(|(_, t)| r32(*t))
                    {
                        if t < dist {
                            dist = t;
                        }
                        let delta = dir * self.config.push_distance;
                        if let Some(sender) = self.senders.get_mut(&id) {
                            sender.send(ServerMessage::YouWasPushed(delta));
                            self.wait_for_teleport_ack.insert(id);
                        }
                        let player_pos = self.player_pos.get_mut(&id).unwrap();
                        let damage_pos = pos.pos + dir.extend(0.0) * (t + 1.0);
                        player_pos.pos += delta.extend(0.0);
                        for (&other_id, other) in &mut self.senders {
                            other.send(ServerMessage::Damage(damage_pos));
                            if other_id != id {
                                other.send(ServerMessage::WasPushed(id, *player_pos));
                            }
                        }
                    }
                    let new_pos = pos.pos + dir.extend(0.0) * dist;

                    if let Some(sender) = self.senders.get_mut(&client) {
                        sender.send(ServerMessage::YouDash(new_pos));
                        self.wait_for_teleport_ack.insert(client);
                        self.player_pos.get_mut(&client).unwrap().pos = new_pos;
                        self.dash_cooldowns
                            .insert(client, self.config.dash_cooldown);
                        for (&id, other) in &mut self.senders {
                            if id != client {
                                other.send(ServerMessage::Dash(
                                    client,
                                    *self.player_pos.get(&client).unwrap(),
                                ));
                            }
                        }
                    }
                }
                false
            }
        });

        if let Some(timer) = &mut self.restart_timer {
            *timer -= delta_time;
            if *timer < 0.0 {
                self.restart_timer = None;
                self.restart();
            }
        } else if (self.player_pos.len() <= 1 && self.names.len() >= 2)
            || (self.names.len() == 1 && self.player_pos.is_empty())
        {
            self.restart_timer = Some(self.config.restart_timer);
        }
        for (&shark_id, shark) in &mut self.sharks {
            if let Some(timer) = &mut shark.destroy_timer {
                *timer += delta_time;
                if *timer > 1.0 {
                    shark.destroy_timer = None;
                    let tile = shark.destroy.take().unwrap();
                    self.raft.remove(&tile);

                    for sender in self.senders.values_mut() {
                        sender.send(ServerMessage::Destroy(shark_id, tile));
                    }
                }
                continue;
            }

            let delta = shark.target_pos.extend(self.config.shark.depth) - shark.pos.pos;
            shark.pos.pos += delta.clamp_len(..=delta_time * self.config.shark.speed);
            if delta.len() < 1e-5 {
                if let Some(tile) = shark.destroy {
                    if self.raft.contains(&tile) {
                        for sender in self.senders.values_mut() {
                            sender.send(ServerMessage::AboutToDestroy(shark_id, tile));
                        }
                        shark.destroy_timer = Some(0.0);
                    } else {
                        shark.destroy = None;
                    }
                    continue;
                }
                let bb = Aabb2::points_bounding_box(self.raft.iter().copied())
                    .map_or(Aabb2::ZERO, |bb| bb.map(|x| x as f32));
                let center = bb.center();
                let r = partial_max(bb.width(), bb.height()) / 2.0 * self.config.tile_size;

                if thread_rng().gen_bool(self.config.shark.attack_prob) && !self.raft.is_empty() {
                    let (tile, empty) = self
                        .raft
                        .iter()
                        .copied()
                        .filter_map(|tile| {
                            [vec2(-1, 0), vec2(1, 0), vec2(0, -1), vec2(0, 1)]
                                .into_iter()
                                .map(|d| tile + d)
                                .find(|&next| !self.raft.contains(&next))
                                .map(|next| (tile, next))
                        })
                        .choose(&mut thread_rng())
                        .unwrap();
                    shark.target_pos = empty.map(|x| x as f32) * self.config.tile_size;
                    shark.destroy = Some(tile);
                } else {
                    shark.target_pos =
                        thread_rng().gen_circle(center, r + self.config.shark.extra_move_radius);
                }
            } else {
                shark.pos.vel = delta.normalize_or_zero() * self.config.shark.speed;
                shark.pos.rot = delta.xy().arg();
            }
        }
    }
}

pub struct App {
    state: Arc<Mutex<State>>,
}

impl App {
    const TPS: f32 = 10.0;
    pub fn new() -> Self {
        let config = futures::executor::block_on(file::load_detect(
            run_dir().join("assets").join("config.toml"),
        ))
        .unwrap();
        let state = Arc::new(Mutex::new(State::new(config)));
        std::thread::spawn({
            let state = state.clone();
            move || loop {
                let delta_time = 1.0 / Self::TPS;
                {
                    let mut state = state.lock().unwrap();
                    if state.should_exit {
                        break;
                    }
                    state.tick(delta_time);
                }
                std::thread::sleep(std::time::Duration::from_secs_f32(delta_time));
            }
        });
        Self { state }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.state.lock().unwrap().should_exit = true;
    }
}

pub struct ClientConnection {
    id: Id,
    state: Arc<Mutex<State>>,
}

impl geng::net::Receiver<ClientMessage> for ClientConnection {
    fn handle(&mut self, message: ClientMessage) {
        self.state.lock().unwrap().handle(self.id, message);
    }
}

impl Drop for ClientConnection {
    fn drop(&mut self) {
        self.state.lock().unwrap().drop_player(self.id);
    }
}

impl geng::net::server::App for App {
    type Client = ClientConnection;
    type ServerMessage = ServerMessage;
    type ClientMessage = ClientMessage;
    fn connect(&mut self, sender: Box<dyn geng::net::Sender<Self::ServerMessage>>) -> Self::Client {
        let id = self.state.lock().unwrap().new_player(sender);
        ClientConnection {
            id,
            state: self.state.clone(),
        }
    }
}

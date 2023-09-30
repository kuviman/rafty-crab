use super::*;

struct State {
    should_exit: bool,
    config: assets::Config,
    id_gen: IdGen,
    player_pos: HashMap<Id, Pos>,
    raft: HashSet<vec2<i32>>,
    senders: HashMap<Id, Box<dyn geng::net::Sender<ServerMessage>>>,
    sharks: HashMap<Id, Shark>,
    restart_timer
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
        self.raft = Aabb2::ZERO
            .extend_uniform(self.config.raft_size)
            .extend_positive(vec2::splat(1))
            .points()
            .filter(|tile| tile.map(|x| x as f32).len() <= self.config.raft_size as f32 + 0.5)
            .collect();
        for (&client, sender) in &mut self.senders {
            sender.send(ServerMessage::UpdateRaft(self.raft.clone()));

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
            player_pos: default(),
            senders: default(),
            raft: default(),
            should_exit: false,
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
        self.senders.insert(id, sender);
        id
    }
    pub fn drop_player(&mut self, client: Id) {
        self.player_pos.remove(&client);
        self.senders.remove(&client);
        for sender in self.senders.values_mut() {
            sender.send(ServerMessage::PlayerLeft { id: client });
        }
    }
    pub fn handle(&mut self, client: Id, message: ClientMessage) {
        let sender = self.senders.get_mut(&client).unwrap();
        match message {
            ClientMessage::UpdatePos(pos) => {
                if let std::collections::hash_map::Entry::Occupied(mut e) =
                    self.player_pos.entry(client)
                {
                    e.insert(pos);
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
                        sender.send(ServerMessage::YouDrown);
                        for (&id, other) in &mut self.senders {
                            if id == client {
                                continue;
                            }
                            other.send(ServerMessage::PlayerDrown(client));
                        }
                    }
                }
            }
            ClientMessage::Pig => {
                sender.send(ServerMessage::Pog);
                for (&id, &pos) in &self.player_pos {
                    if id != client {
                        sender.send(ServerMessage::UpdatePos { id, pos });
                    }
                }
                sender.send(ServerMessage::UpdateSharks(self.sharks.clone()));
            }
        }
    }
    fn tick(&mut self, delta_time: f32) {
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

use super::*;

#[derive(Deserialize)]
pub struct Config {
    raft_size: i32,
}

struct State {
    config: Config,
    last_id: Id,
    player_pos: HashMap<Id, Pos>,
    raft: HashSet<vec2<i32>>,
    senders: HashMap<Id, Box<dyn geng::net::Sender<ServerMessage>>>,
}

impl State {
    fn new(config: Config) -> Self {
        Self {
            player_pos: default(),
            last_id: 0,
            senders: default(),
            raft: Aabb2::ZERO
                .extend_uniform(config.raft_size)
                .extend_positive(vec2::splat(1))
                .points()
                .filter(|tile| tile.map(|x| x as f32).len() <= config.raft_size as f32 + 0.5)
                .collect(),
            config,
        }
    }
    pub fn new_player(&mut self, mut sender: Box<dyn geng::net::Sender<ServerMessage>>) -> Id {
        self.last_id += 1;
        let id = self.last_id;
        let pos = Pos {
            pos: vec3(
                thread_rng().gen_range(-1.0..=1.0),
                thread_rng().gen_range(-1.0..=1.0),
                0.0,
            ),
            rot: Angle::from_degrees(thread_rng().gen_range(0.0..360.0)),
            vel: vec3::ZERO,
        };
        self.player_pos.insert(id, pos);
        for sender in self.senders.values_mut() {
            sender.send(ServerMessage::NewPlayer { id, pos });
        }
        sender.send(ServerMessage::YouSpawn(Spawn { pos }));
        sender.send(ServerMessage::Pog);
        for (&other_id, &pos) in &self.player_pos {
            if other_id != id {
                sender.send(ServerMessage::NewPlayer { id: other_id, pos });
            }
        }
        sender.send(ServerMessage::UpdateRaft(self.raft.clone()));
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
        match message {
            ClientMessage::UpdatePos(pos) => {
                self.player_pos.insert(client, pos);
            }
            ClientMessage::Pig => {
                let sender = self.senders.get_mut(&client).unwrap();
                sender.send(ServerMessage::Pog);
                for (&id, &pos) in &self.player_pos {
                    if id != client {
                        sender.send(ServerMessage::UpdatePos { id, pos });
                    }
                }
            }
        }
    }
}

pub struct App {
    state: Arc<Mutex<State>>,
}

impl App {
    pub fn new() -> Self {
        let config = futures::executor::block_on(file::load_detect(
            run_dir().join("assets").join("server_config.toml"),
        ))
        .unwrap();
        Self {
            state: Arc::new(Mutex::new(State::new(config))),
        }
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

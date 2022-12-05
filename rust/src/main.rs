use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc::{self, UnboundedSender};

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

enum ClientRequest<'a> {
    Register {
        username: &'a str,
        password: &'a str,
    },
    Login {
        username: &'a str,
        password: &'a str,
    },
    Join {
        channel: &'a str,
    },
    Create {
        channel: &'a str,
    },
    Say {
        channel: &'a str,
        message: &'a str,
    },
    Channels,
}

enum ServerResult<'a> {
    Join {
        user: &'a str,
        channel: &'a str,
        status: &'a str,
    },
    Say {
        user: &'a str,
        channel: &'a str,
        status: &'a str,
        msg: &'a str,
    },
}

enum ServerRequest<'a> {
    Out,
    Confirm,
    Channels {
        channels: &'a str,
    },
    New {
        channel: &'a str,
    },
    Join {
        user: &'a str,
        channel: &'a str,
    },
    Say {
        user: &'a str,
        channel: &'a str,
        msg: &'a str,
    },
    Recv {
        to_user: &'a str,
        from_user: &'a str,
        channel: &'a str,
        msg: &'a str,
    },
    Result(ServerResult<'a>),
}

enum Request<'a> {
    Client(ClientRequest<'a>),
    Server(ServerRequest<'a>),
}

fn two(input: &str) -> Option<(&str, &str)> {
    input.split_once(' ').filter(|(_, b)| !b.contains(' '))
}

fn parse_client(input: &str) -> Option<ClientRequest<'_>> {
    use ClientRequest::*;

    let (kind, args) = input.split_once(' ').unwrap_or((&*input, ""));
    let req = match kind {
        "REGISTER" => {
            let (username, password) = two(args)?;
            Register { username, password }
        }
        "LOGIN" => {
            let (username, password) = two(args)?;
            Login { username, password }
        }
        "JOIN" => {
            if args.contains(' ') {
                return None;
            }
            Join { channel: args }
        }
        "CREATE" => {
            if args.contains(' ') {
                return None;
            }
            Create { channel: args }
        }
        "SAY" => {
            let (channel, message) = args.split_once(' ')?;
            Say { channel, message }
        }
        "CHANNELS" => Channels,
        _ => return None,
    };

    Some(req)
}

fn parse_server(input: &str) -> Option<ServerRequest<'_>> {
    use ServerRequest::*;

    let (kind, args) = input.split_once(' ').unwrap_or((&*input, ""));
    let req = match kind {
        "FEDOUT" => Out,
        "FEDCONFIRM" => Confirm,
        "FEDCHANNELS" => Channels { channels: args },
        "FEDNEW" => {
            if args.contains(' ') {
                return None;
            }
            New { channel: args }
        }
        "FEDJOIN" => {
            let (user, channel) = two(args)?;
            Join { user, channel }
        }
        "FEDSAY" => {
            let (user, args) = args.split_once(' ')?;
            let (channel, msg) = args.split_once(' ')?;
            Say { user, channel, msg }
        }
        "FEDRECV" => {
            let (to_user, args) = args.split_once(' ')?;
            let (from_user, args) = args.split_once(' ')?;
            let (channel, msg) = args.split_once(' ')?;
            Recv {
                to_user,
                from_user,
                channel,
                msg,
            }
        }
        "FEDRESULT" => {
            let (user, args) = args.split_once(' ')?;
            let (kind, args) = args.split_once(' ')?;
            let (channel, args) = args.split_once(' ')?;
            match kind {
                "JOIN" => {
                    if !matches!(args, "0" | "1") {
                        return None;
                    }
                    Result(ServerResult::Join {
                        user,
                        channel,
                        status: args,
                    })
                }
                "SAY" => {
                    let (status, msg) = args.split_once(' ')?;
                    if !matches!(status, "0" | "1") {
                        return None;
                    }
                    Result(ServerResult::Say {
                        user,
                        channel,
                        status,
                        msg,
                    })
                }
                _ => return None,
            }
        }
        _ => return None,
    };

    Some(req)
}

fn parse(input: &str) -> Option<Request<'_>> {
    parse_client(input)
        .map(Request::Client)
        .or_else(|| parse_server(input).map(Request::Server))
}

type ClientChannel = Arc<UnboundedSender<Arc<String>>>;

struct ClientConnection {
    username: Option<Arc<String>>,
    channel: ClientChannel,
    server_addr: SocketAddr,
}

#[derive(PartialEq, Eq, Hash, Debug)]
enum Response {
    Join { channel: String },
    Say { channel: String, message: String },
}

#[derive(Debug)]
enum ServerMessage {
    Message(Arc<String>),
    CallbackMessage {
        channel: ClientChannel,
        user: Arc<String>,
        response: Response,
        message: String,
    },
}

type ServerChannel = Arc<UnboundedSender<ServerMessage>>;

struct ServerConnection {
    channel: ServerChannel,
    server_addr: SocketAddr,
    callbacks: HashMap<(Arc<String>, Response), ClientChannel>,
}

struct RemoteServer {
    channel: ServerChannel,
    channels: RwLock<HashSet<String>>,
}

enum User {
    Local(ClientChannel),
    Remote(ServerChannel),
}

struct Channel {
    users: HashMap<Arc<String>, User>,
}

struct Server {
    port: u16,
    users: RwLock<HashMap<Arc<String>, String>>,
    user_conns: RwLock<HashMap<Arc<String>, ClientChannel>>,
    channels: RwLock<HashMap<String, RwLock<Channel>>>,
    servers: RwLock<HashMap<SocketAddr, RemoteServer>>,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Server {
            port,
            users: Default::default(),
            user_conns: Default::default(),
            channels: Default::default(),
            servers: Default::default(),
        }
    }
}

fn register(server: &Server, username: &str, password: &str) -> String {
    let username = username.to_string();
    // read
    {
        if server.users.read().unwrap().contains_key(&username) {
            return String::from("RESULT REGISTER 0\n");
        }
    }
    // write
    {
        server
            .users
            .write()
            .unwrap()
            .insert(Arc::new(username), password.to_string());
    }
    String::from("RESULT REGISTER 1\n")
}

fn login(server: &Server, conn: &mut ClientConnection, username: &str, password: &str) -> String {
    let username = username.to_string();
    if let Some((un, pass)) = server.users.read().unwrap().get_key_value(&username) {
        if pass == password {
            conn.username = Some(Arc::clone(un));
            return String::from("RESULT LOGIN 1\n");
        }
    }
    String::from("RESULT LOGIN 0\n")
}

fn join(server: &Server, conn: &mut ClientConnection, channel: &str) -> String {
    fn _join(server: &Server, conn: &mut ClientConnection, channel: &str) -> Option<()> {
        let username = conn.username.as_ref()?;

        let channels = server.channels.read().unwrap();
        match channel.split_once(':') {
            Some((channel, remote)) => {
                let remote: SocketAddr = remote.parse().ok()?;
                let servers = server.servers.read().unwrap();
                let remote = servers.get(&remote)?;
                let message = format!("FEDJOIN {}@{} {}\n", username, conn.server_addr, channel);
                let message = ServerMessage::CallbackMessage {
                    channel: Arc::clone(&conn.channel),
                    user: Arc::clone(username),
                    response: Response::Join {
                        channel: channel.to_string(),
                    },
                    message,
                };
                remote.channel.send(message).unwrap();
                None
            }
            None => {
                let c = channels.get(channel)?;
                // read
                {
                    if c.read().unwrap().users.contains_key(&*username) {
                        return None;
                    }
                }
                // write
                {
                    c.write()
                        .unwrap()
                        .users
                        .insert(Arc::clone(username), User::Local(Arc::clone(&conn.channel)));
                }
                Some(())
            }
        }
    }

    let status = _join(server, conn, channel).map_or(0, |_| 1);
    format!("RESULT JOIN {} {}\n", channel, status)
}

fn create(server: &Server, channel: &str) -> String {
    // read
    {
        if server.channels.read().unwrap().contains_key(channel) {
            // fail
            return format!("RESULT CREATE {} 0\n", channel);
        }
    }
    // write
    {
        server.channels.write().unwrap().insert(
            channel.to_string(),
            RwLock::new(Channel {
                users: Default::default(),
            }),
        );
    }
    // alert
    {
        let alert = Arc::new(format!("FEDNEW {}\n", channel));
        for remote in server.servers.read().unwrap().values() {
            remote
                .channel
                .send(ServerMessage::Message(Arc::clone(&alert)))
                .unwrap();
        }
    }
    format!("RESULT CREATE {} 1\n", channel)
}

fn _say(server: &Server, username: &String, channel_name: &str, msg: &str) -> bool {
    let channels = server.channels.read().unwrap();
    let Some(c) = channels.get(channel_name) else { return false };
    let users = &c.read().unwrap().users;

    if users.contains_key(username) {
        let local_message = Arc::new(format!("RECV {} {} {}\n", username, channel_name, msg));
        for (name, user) in users {
            // @Speed currently we are using an unbounded channel so we don't have to await in
            // this loop while holding a read lock on users
            // There may also be a deadlock here if we have two users trying to talk to
            // each other and this is a bounded channel
            match user {
                User::Local(channel) => channel.send(Arc::clone(&local_message)).unwrap(),
                User::Remote(channel) => {
                    let remote_message = Arc::new(format!(
                        "FEDRECV {} {} {} {}\n",
                        name, username, channel_name, msg
                    ));
                    channel
                        .send(ServerMessage::Message(remote_message))
                        .unwrap()
                }
            }
        }
        true
    } else {
        false
    }
}

fn say(server: &Server, conn: &mut ClientConnection, channel: &str, msg: &str) -> String {
    let status = conn
        .username
        .as_ref()
        .map(|un| _say(server, un, channel, msg))
        .unwrap_or(false);
    format!("RESULT SAY {} {}\n", channel, status as i8)
}

fn list_channels(server: &Server, s: &mut String) {
    let channels = server.channels.read().unwrap();
    for name in channels.keys() {
        s.push(' ');
        s.push_str(name);
        s.push(',');
    }
    if !channels.is_empty() {
        s.pop();
    }
    s.push('\n');
}

fn channels(server: &Server) -> String {
    let mut s = String::from("RESULT CHANNELS");
    list_channels(server, &mut s);
    s
}

fn fed_out(server: &Server, conn: &mut ServerConnection) -> Option<String> {
    server.servers.write().unwrap().insert(
        conn.server_addr,
        RemoteServer {
            channel: Arc::clone(&conn.channel),
            channels: Default::default(),
        },
    );
    Some(String::from("FEDCONFIRM\n"))
}

fn fed_confirm(server: &Server, conn: &mut ServerConnection) -> Option<String> {
    server.servers.write().unwrap().insert(
        conn.server_addr,
        RemoteServer {
            channel: Arc::clone(&conn.channel),
            channels: Default::default(),
        },
    );
    let mut s = String::from("FEDCHANNELS");
    list_channels(server, &mut s);
    Some(s)
}

fn fed_channels(server: &Server, conn: &mut ServerConnection, channels: &str) -> Option<String> {
    let servers = server.servers.read().unwrap();
    let remote = if let Some(r) = servers.get(&conn.server_addr) {
        r
    } else {
        panic!();
    };

    for channel in channels.split(", ") {
        remote.channels.write().unwrap().insert(channel.to_string());
    }
    None
}

fn fed_new(server: &Server, conn: &mut ServerConnection, channel: &str) -> Option<String> {
    let servers = server.servers.read().unwrap();
    let remote = if let Some(r) = servers.get(&conn.server_addr) {
        r
    } else {
        panic!();
    };

    remote.channels.write().unwrap().insert(channel.to_string());
    None
}

fn fed_join(
    server: &Server,
    conn: &mut ServerConnection,
    user: &str,
    channel: &str,
) -> Option<String> {
    fn _join(server: &Server, conn: &mut ServerConnection, user: &str, channel: &str) -> bool {
        let channels = server.channels.read().unwrap();
        let Some(c) = channels.get(channel) else { return false };
        let user = user.to_string();
        // read
        {
            if c.read().unwrap().users.contains_key(&user) {
                return false;
            }
        }
        // write
        {
            c.write()
                .unwrap()
                .users
                .insert(Arc::new(user), User::Remote(Arc::clone(&conn.channel)));
        }
        true
    }

    let status = _join(server, conn, user, channel);
    Some(format!(
        "FEDRESULT {} JOIN {} {}\n",
        user, channel, status as i8
    ))
}

fn fed_say(server: &Server, user: &str, channel: &str, msg: &str) -> Option<String> {
    let status = _say(server, &user.to_string(), user, channel);
    Some(format!(
        "FEDRESULT {} SAY {} {} {}\n",
        user, channel, status as i8, msg
    ))
}

fn fed_recv(
    server: &Server,
    to_user: &str,
    from_user: &str,
    channel: &str,
    msg: &str,
) -> Option<String> {
    if let Some(conn) = server.user_conns.read().unwrap().get(&to_user.to_string()) {
        conn.send(Arc::new(format!(
            "RECV {} {} {}\n",
            from_user, channel, msg
        )))
        .unwrap()
    }

    None
}

fn fed_result_join(conn: &mut ServerConnection, user: &str, channel: &str, status: &str) {
    let key = (
        Arc::new(user.to_string()),
        Response::Join {
            channel: channel.to_string(),
        },
    );
    if let Some(sender) = conn.callbacks.remove(&key) {
        sender
            .send(Arc::new(format!("RESULT JOIN {} {}\n", channel, status)))
            .unwrap();
    }
}

fn fed_result_say(conn: &mut ServerConnection, user: &str, channel: &str, status: &str, msg: &str) {
    let key = (
        Arc::new(user.to_string()),
        Response::Say {
            channel: channel.to_string(),
            message: msg.to_string(),
        },
    );
    if let Some(sender) = conn.callbacks.remove(&key) {
        sender
            .send(Arc::new(format!(
                "RESULT SAY {} {} {}\n",
                channel, msg, status
            )))
            .unwrap();
    }
}

async fn process_server_request(
    server: &Server,
    conn: &mut ServerConnection,
    writer: &mut OwnedWriteHalf,
    req: ServerRequest<'_>,
) {
    let msg = match req {
        ServerRequest::Out => fed_out(server, conn),
        ServerRequest::Confirm => fed_confirm(server, conn),
        ServerRequest::Channels { channels } => fed_channels(server, conn, channels),
        ServerRequest::New { channel } => fed_new(server, conn, channel),
        ServerRequest::Join { user, channel } => fed_join(server, conn, user, channel),
        ServerRequest::Say { user, channel, msg } => fed_say(server, user, channel, msg),
        ServerRequest::Recv {
            to_user,
            from_user,
            channel,
            msg,
        } => fed_recv(server, to_user, from_user, channel, msg),
        ServerRequest::Result(res) => {
            match res {
                ServerResult::Join {
                    user,
                    channel,
                    status,
                } => fed_result_join(conn, user, channel, status),
                ServerResult::Say {
                    user,
                    channel,
                    status,
                    msg,
                } => fed_result_say(conn, user, channel, status, msg),
            }
            None
        }
    };
    if let Some(msg) = msg {
        writer.write_all(msg.as_bytes()).await.unwrap();
    }
}

async fn process_server(
    server: &Server,
    mut lines: Lines<BufReader<OwnedReadHalf>>,
    mut writer: OwnedWriteHalf,
    mut shutdown: Shutdown,
    inital_request: ServerRequest<'_>,
) {
    let addr = lines.get_ref().get_ref().local_addr().unwrap();
    let (sender, mut receiver) = mpsc::unbounded_channel::<ServerMessage>();

    let mut connection = ServerConnection {
        channel: Arc::new(sender),
        server_addr: addr,
        callbacks: Default::default(),
    };

    process_server_request(server, &mut connection, &mut writer, inital_request).await;

    loop {
        tokio::select! {
            Some(line) = async { lines.next_line().await.unwrap() } => {
                let req = match parse_server(&line) {
                    Some(r) => r,
                    None => continue,
                };
                process_server_request(server, &mut connection, &mut writer, req).await;
            },
            Some(msg) = receiver.recv() => {
                match msg {
                    ServerMessage::Message(msg) => {
                        writer.write_all(msg.as_bytes()).await.unwrap();
                    }
                    ServerMessage::CallbackMessage { channel, user, response, message } => {
                        writer.write_all(message.as_bytes()).await.unwrap();
                        connection.callbacks.insert((user, response), channel);
                    }
                }
            },
            _ = shutdown.shutdown.recv() => break,
            // @Todo this has to be wrong
            else => break,
        }
    }
}

async fn process_client_request(
    server: &Server,
    conn: &mut ClientConnection,
    writer: &mut OwnedWriteHalf,
    req: ClientRequest<'_>,
) {
    let msg = match req {
        ClientRequest::Register { username, password } => register(server, username, password),
        ClientRequest::Login { username, password } => login(server, conn, username, password),
        ClientRequest::Join { channel } => join(server, conn, channel),
        ClientRequest::Create { channel } => create(server, channel),
        ClientRequest::Say { channel, message } => say(server, conn, channel, message),
        ClientRequest::Channels => channels(server),
    };
    writer.write_all(msg.as_bytes()).await.unwrap();
}

async fn process_client(
    server: &Server,
    mut lines: Lines<BufReader<OwnedReadHalf>>,
    mut writer: OwnedWriteHalf,
    mut shutdown: Shutdown,
    initial_request: ClientRequest<'_>,
) {
    let addr = lines.get_ref().get_ref().local_addr().unwrap();
    let (sender, mut receiver) = mpsc::unbounded_channel::<Arc<String>>();

    let mut connection = ClientConnection {
        username: None,
        channel: Arc::new(sender),
        server_addr: addr,
    };

    process_client_request(server, &mut connection, &mut writer, initial_request).await;

    loop {
        tokio::select! {
            Some(line) = async { lines.next_line().await.unwrap() } => {
                let req = match parse_client(&line) {
                    Some(r) => r,
                    None => continue,
                };
                process_client_request(server, &mut connection, &mut writer, req).await;
            },
            Some(msg) = receiver.recv() => {
                writer.write_all(msg.as_bytes()).await.unwrap();
            },
            _ = shutdown.shutdown.recv() => break,
            else => break,
        }
    }
}

async fn process(server: &Server, socket: TcpStream, mut shutdown: Shutdown) {
    let (reader, writer) = socket.into_split();
    let mut lines = BufReader::new(reader).lines();

    tokio::select! {
        line = async { lines.next_line().await.unwrap().unwrap() } => {
            let req = match parse(&line) {
                Some(r) => r,
                None => panic!(),
            };
            match req {
                Request::Client(r) => process_client(server, lines, writer, shutdown, r).await,
                Request::Server(r) => process_server(server, lines, writer, shutdown, r).await,
            }
        }
        _ = shutdown.shutdown.recv() => return,
    }
}

struct Shutdown {
    _sender: mpsc::Sender<()>,
    shutdown: broadcast::Receiver<()>,
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .expect("Port number")
        .parse()
        .expect("Provided port is a valid number");
    let server = Arc::new(Server::new(port));
    let listener = TcpListener::bind(("127.0.0.1", server.port)).await.unwrap();

    let (task_send, mut task_recv) = mpsc::channel(1);
    let (shutdown_send, _) = broadcast::channel(1);

    if let Some(file) = std::env::args().nth(2) {
        let string = std::fs::read_to_string(file).expect("Invalid configuration file path");
        for line in string.lines() {
            let server = Arc::clone(&server);
            let line = line.to_string();
            let shutdown = Shutdown {
                _sender: task_send.clone(),
                shutdown: shutdown_send.subscribe(),
            };
            tokio::spawn(async move {
                match TcpStream::connect(&line).await {
                    Ok(mut socket) => {
                        socket.write_all(b"FEDERATEOUT\n").await.unwrap();
                        process(&*server, socket, shutdown).await
                    }
                    Err(e) => eprintln!("Failed to connect to server at {}: {}", line, e),
                }
            });
        }
    }

    loop {
        tokio::select! {
            (socket, _) = async { listener.accept().await.unwrap() } => {
                let server = Arc::clone(&server);
                let shutdown = Shutdown {
                    _sender: task_send.clone(),
                    shutdown: shutdown_send.subscribe(),
                };
                tokio::spawn(async move {
                    process(&*server, socket, shutdown).await;
                });
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }

    drop(task_send);
    drop(shutdown_send);
    let _ = task_recv.recv().await;
    println!("Shut Down cleanly!")
}

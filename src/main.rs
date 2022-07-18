use argh::FromArgs;
use mpvipc::*;
use rand::Rng;
use rumqttc::{Client, Connection, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::{
    io,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Network messages
pub enum ProtoMessage {
    /// Sent when joining a room
    Join(String),
    Play(f64),
    Pause,
    Media(String),
}

/// Player messages
#[derive(Debug)]
pub enum VideoMessage {
    Seek(f64),
    Unpause,
    Pause,
    Media(String),
}

trait VideoPlayer {
    fn run(&self, autostart: bool, rx: mpsc::Receiver<VideoMessage>);
}

struct MpvPlayer {
    ipc_path: String,
}

impl VideoPlayer for MpvPlayer {
    fn run(&self, autostart: bool, rx: mpsc::Receiver<VideoMessage>) {
        if autostart {
            let ipc_arg = format!("{}={}", "--input-ipc-server", self.ipc_path);

            Command::new("mpv")
                .arg(ipc_arg)
                .arg("--idle")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                // keep stderr
                .spawn()
                .unwrap();

            // ew
            thread::sleep(Duration::from_millis(500));
        }

        let mpv = Mpv::connect(&self.ipc_path).unwrap_or_else(|e| {
            println!("error connecting to mpv, is mpv running?");
            println!(
                r#"start with "mpv --input-ipc-server={} --idle""#,
                self.ipc_path
            );
            panic!("{}", e);
        });

        for msg in rx.iter() {
            println!("mpv: {:?}", &msg);
            match msg {
                VideoMessage::Pause => {
                    mpv.pause().expect("failed to pause");
                }
                VideoMessage::Unpause => {
                    mpv.set_property("pause", false).expect("failed to unpause");
                }
                VideoMessage::Seek(pos) => {
                    mpv.seek(pos, SeekOptions::Absolute)
                        .expect("failed to seek");
                }
                VideoMessage::Media(path) => {
                    mpv.playlist_add(
                        &path,
                        PlaylistAddTypeOptions::File,
                        PlaylistAddOptions::Append,
                    )
                    .unwrap();

                    let playlist = mpv.get_playlist().unwrap();
                    let entry = playlist
                        .0
                        .iter()
                        .rev()
                        .find(|entry| entry.filename == path)
                        .unwrap();

                    mpv.playlist_play_id(entry.id).unwrap();
                    // but start off paused
                    mpv.pause().unwrap();
                }
            }
        }
    }
}

#[derive(FromArgs, Debug)]
/// Tool for syncing video playback
struct Args {
    #[argh(switch)]
    /// autostart mpv
    autostart: bool,

    #[argh(
        option,
        long = "server",
        default = r#"String::from("test.mosquitto.org")"#
    )]
    /// server ip/domain
    server: String,

    #[argh(option, long = "port", default = "1883")]
    /// server port
    port: u16,

    #[argh(option, long = "room", default = r#"String::from("default_room")"#)]
    /// name of room
    room: String,

    #[argh(option, long = "ipc", default = r#"String::from("/tmp/mpv.sock")"#)]
    /// path to mpv socket
    ipc_path: String,

    #[argh(switch)]
    /// send fake requests for testing
    spoof: bool,
}

fn main() {
    let args: Args = argh::from_env();
    let topic = format!("{}/{}", "uniplay", args.room);
    let user_id = {
        let mut rng = rand::thread_rng();
        let id: u32 = rng.gen();
        format!("uniplayuser{}", id)
    };

    let (pt_tx, pt_rx) = mpsc::channel::<ProtoMessage>();
    let (vd_tx, vd_rx) = mpsc::channel::<VideoMessage>();

    let relay_handle = thread::spawn(move || relay(pt_rx, vd_tx));

    let mut mqttoptions = MqttOptions::new(&user_id, args.server, args.port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (mut client, connection) = Client::new(mqttoptions, 10);
    client.subscribe(&topic, QoS::ExactlyOnce).unwrap();

    let mpv = MpvPlayer {
        ipc_path: args.ipc_path,
    };
    let mpv_handle = thread::spawn(move || mpv.run(args.autostart, vd_rx));

    thread::spawn(|| {
        mqtt_listen(connection, pt_tx);
    });

    thread::spawn(move || {
        let msg = serde_json::to_string(&ProtoMessage::Join(user_id)).unwrap();
        client
            .publish(&topic, QoS::ExactlyOnce, false, msg)
            .unwrap();

        if args.spoof {
            mqtt_spoof(client, &topic);
        } else {
            repl(client, &topic);
        }
    });

    mpv_handle.join().unwrap();
    relay_handle.join().unwrap();
}

fn mqtt_listen(mut connection: Connection, tx: mpsc::Sender<ProtoMessage>) {
    use rumqttc::{Event, Packet};
    for msg in connection
        .iter()
        .map(Result::unwrap)
        .filter_map(|notification| {
            if let Event::Incoming(v) = notification {
                Some(v)
            } else {
                None
            }
        })
        .filter_map(|incoming| {
            if let Packet::Publish(p) = incoming {
                Some(p)
            } else {
                None
            }
        })
        .map(|publish| serde_json::from_slice(&publish.payload).unwrap())
    {
        tx.send(msg).unwrap();
    }
}

fn relay(rx: mpsc::Receiver<ProtoMessage>, tx: mpsc::Sender<VideoMessage>) {
    for msg in rx.iter() {
        println!("relay: {:?}", msg);
        match msg {
            ProtoMessage::Join(name) => {
                println!("{} joined the room.", name);
            }
            ProtoMessage::Play(pos) => {
                tx.send(VideoMessage::Seek(pos)).unwrap();
                tx.send(VideoMessage::Unpause).unwrap();
            }
            ProtoMessage::Pause => {
                tx.send(VideoMessage::Pause).unwrap();
            }
            ProtoMessage::Media(link) => {
                tx.send(VideoMessage::Media(link)).unwrap();
            }
        }
    }
}

fn repl(mut client: Client, topic: &String) {
    println!("commands: [set <link>, play <seconds>, pause]");

    let stdin = io::stdin();
    let mut input = String::new();
    while stdin.read_line(&mut input).is_ok() {
        let (keyword, arg) = {
            let mut words = input.trim().split(' ');
            (words.next().unwrap(), words.next().unwrap_or(""))
        };

        let msg = match keyword {
            "set" => Some(ProtoMessage::Media(arg.to_string())),
            "pause" => Some(ProtoMessage::Pause),
            "play" => Some(ProtoMessage::Play(arg.parse().unwrap())),
            _ => {
                println!("unknown command");
                None
            }
        };

        if let Some(msg) = msg {
            let msg = serde_json::to_string(&msg).unwrap();
            client.publish(topic, QoS::ExactlyOnce, false, msg).unwrap();
        }

        input = String::new();
    }
}

fn mqtt_spoof(mut client: Client, topic: &String) {
    let messages = vec![
        ProtoMessage::Media("https://youtu.be/jNQXAC9IVRw".to_string()),
        ProtoMessage::Play(2.0),
        ProtoMessage::Pause,
        ProtoMessage::Play(4.0),
    ];

    thread::sleep(Duration::from_millis(500));
    for msg in messages {
        let msg = serde_json::to_string(&msg).unwrap();

        client.publish(topic, QoS::ExactlyOnce, false, msg).unwrap();

        thread::sleep(Duration::from_millis(10_000));
    }
}

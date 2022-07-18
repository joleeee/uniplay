use argh::FromArgs;
use mpvipc::*;
use rand::Rng;
use rumqttc::{Client, Connection, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::{io, process::Command, sync::mpsc, thread, time::Duration};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Play(f64),
    Pause,
    Media(String),
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

    if args.autostart {
        let ipc_arg = format!("{}={}", "--input-ipc-server", args.ipc_path);

        Command::new("mpv")
            .arg(ipc_arg)
            .arg("--idle")
            .spawn()
            .unwrap();

        // ew
        thread::sleep(Duration::from_millis(500));
    }

    let (tx, rx) = mpsc::channel::<Message>();
    let mpv_handle = thread::spawn(move || mpv(rx, &args.ipc_path));

    let mut mqttoptions = MqttOptions::new(user_id, args.server, args.port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (mut client, connection) = Client::new(mqttoptions, 10);
    client.subscribe(&topic, QoS::ExactlyOnce).unwrap();

    thread::spawn(|| {
        mqtt_listen(connection, tx);
    });

    thread::spawn(move || {
        if args.spoof {
            mqtt_spoof(client, &topic);
        } else {
            repl(client, &topic);
        }
    });

    mpv_handle.join().unwrap();
}

fn mqtt_listen(mut connection: Connection, tx: mpsc::Sender<Message>) {
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

fn mpv(rx: mpsc::Receiver<Message>, ipc_path: &str) {
    let mpv = Mpv::connect(ipc_path).unwrap_or_else(|e| {
        println!("error connecting to mpv, is mpv running?");
        println!(r#"start with "mpv --input-ipc-server={} --idle""#, ipc_path,);
        panic!("{}", e);
    });

    for msg in rx.iter() {
        println!("got {:?}", msg);
        match msg {
            Message::Play(pos) => {
                mpv.seek(pos, SeekOptions::Absolute)
                    .expect("play: failed to seek");
                mpv.set_property("pause", false)
                    .expect("play: failed to unpause");
            }
            Message::Pause => mpv.pause().expect("pause: failed to pause"),
            Message::Media(link) => {
                mpv.playlist_add(
                    &link,
                    PlaylistAddTypeOptions::File,
                    PlaylistAddOptions::Append,
                )
                .unwrap();

                let playlist = mpv.get_playlist().unwrap();
                let entry = playlist
                    .0
                    .iter()
                    .rev()
                    .find(|entry| entry.filename == link)
                    .unwrap();

                mpv.playlist_play_id(entry.id).unwrap();
                // but start off paused
                mpv.pause().unwrap();
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
            "set" => Some(Message::Media(arg.to_string())),
            "pause" => Some(Message::Pause),
            "play" => Some(Message::Play(arg.parse().unwrap())),
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
        Message::Media("https://youtu.be/jNQXAC9IVRw".to_string()),
        Message::Play(2.0),
        Message::Pause,
        Message::Play(4.0),
    ];

    thread::sleep(Duration::from_millis(500));
    for msg in messages {
        let msg = serde_json::to_string(&msg).unwrap();

        client.publish(topic, QoS::ExactlyOnce, false, msg).unwrap();

        thread::sleep(Duration::from_millis(10_000));
    }
}

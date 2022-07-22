use argh::FromArgs;
use async_trait::async_trait;
use futures::FutureExt;
use mpvi::{option, Mpv};
use rand::Rng;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tokio::sync::mpsc;

mod cli;
use cli::CliMode;

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Network messages
pub enum ProtoMessage {
    /// Sent when joining a room
    Join(String),
    Chat(String, String),
    PlayFrom(f64),
    Stop,
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

#[async_trait]
trait VideoPlayer {
    fn start(&self) -> std::process::Child;
    async fn run(self, rx: mpsc::Receiver<VideoMessage>);
}

struct MpvPlayer {
    ipc_path: String,
}

#[async_trait]
impl VideoPlayer for MpvPlayer {
    fn start(&self) -> std::process::Child {
        let ipc_arg = format!("{}={}", "--input-ipc-server", self.ipc_path);

        let c = Command::new("mpv")
            .arg(ipc_arg)
            .arg("--idle")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // keep stderr
            .spawn()
            .unwrap();

        // ew
        thread::sleep(Duration::from_millis(400));

        c
    }

    async fn run(self, mut rx: mpsc::Receiver<VideoMessage>) {
        let mpv = Mpv::new(&self.ipc_path).await.unwrap_or_else(|e| {
            println!("error connecting to mpv, is mpv running?");
            println!(
                r#"start with "mpv --input-ipc-server={} --idle""#,
                self.ipc_path
            );
            panic!("{}", e);
        });

        loop {
            let msg = rx.recv().await.expect("closed");
            println!("mpv: {:?}", &msg);
            match msg {
                VideoMessage::Pause => {
                    mpv.pause().await.expect("failed to pause");
                }
                VideoMessage::Unpause => {
                    mpv.unpause().await.expect("failed to unpause");
                }
                VideoMessage::Seek(pos) => {
                    mpv.seek(pos, option::Seek::Absolute)
                        .await
                        .expect("failed to seek");
                }
                VideoMessage::Media(path) => {
                    mpv.load_file(&path, option::Insertion::Replace)
                        .await
                        .expect("failed to load file");

                    // start off paused
                    mpv.pause().await.expect("failed to pause");
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

    /// username
    #[argh(option, long = "name", default = "rnd_name()")]
    name: String,

    #[argh(option, long = "room", default = r#"String::from("default_room")"#)]
    /// name of room
    room: String,

    #[argh(option, long = "ipc", default = r#"String::from("/tmp/mpv.sock")"#)]
    /// path to mpv socket
    ipc_path: String,

    #[argh(option, long = "cli", default = "CliMode::Repl")]
    /// send fake requests for testing
    cli: CliMode,
}

fn rnd_name() -> String {
    let mut rng = rand::thread_rng();
    let id: u32 = rng.gen();
    format!("uniplayuser{}", id)
}

#[tokio::main]
async fn main() {
    let args: Args = argh::from_env();
    let topic = format!("{}/{}", "uniplay", args.room);

    let (pt_tx, pt_rx) = mpsc::channel::<ProtoMessage>(8);
    let (vd_tx, vd_rx) = mpsc::channel::<VideoMessage>(8);

    let mut mqttoptions = MqttOptions::new(&args.name, args.server, args.port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (client, eventloop) = AsyncClient::new(mqttoptions, 10);
    client.subscribe(&topic, QoS::ExactlyOnce).await.unwrap();

    let msg = serde_json::to_string(&ProtoMessage::Join(args.name.clone())).unwrap();
    client
        .publish(&topic, QoS::ExactlyOnce, false, msg.as_bytes())
        .await
        .unwrap();

    tokio::spawn(relay(pt_rx, vd_tx));

    let mpv_player = MpvPlayer {
        ipc_path: args.ipc_path,
    };
    if args.autostart {
        mpv_player.start();
    }
    let mpv_handle = tokio::spawn(mpv_player.run(vd_rx));

    tokio::spawn(mqtt_listen(eventloop, pt_tx));

    tokio::spawn(async move {
        args.cli.run(client, &args.name, &topic).await;
    });

    mpv_handle.await.unwrap();
}

async fn mqtt_listen(mut eventloop: EventLoop, tx: mpsc::Sender<ProtoMessage>) {
    use rumqttc::{Event, Packet};

    fn decode_event(event: Event) -> Option<ProtoMessage> {
        let incoming = if let Event::Incoming(v) = event {
            Some(v)
        } else {
            None
        }?;

        let publish = if let Packet::Publish(p) = incoming {
            Some(p)
        } else {
            None
        }?;

        serde_json::from_slice(&publish.payload).unwrap_or_else(|e| {
            println!("deser error: {}", e);
            None
        })
    }

    loop {
        let msg = eventloop.poll().map(Result::unwrap).map(decode_event).await;

        if let Some(msg) = msg {
            tx.send(msg).await.unwrap();
        }
    }
}

async fn relay(mut rx: mpsc::Receiver<ProtoMessage>, tx: mpsc::Sender<VideoMessage>) {
    loop {
        let msg = rx.recv().await.expect("closed");
        match msg {
            ProtoMessage::Join(name) => {
                println!("{} joined the room.", name);
            }
            ProtoMessage::Chat(from, msg) => {
                println!("<{}> {}", from, msg);
            }
            ProtoMessage::PlayFrom(pos) => {
                tx.send(VideoMessage::Seek(pos)).await.unwrap();
                tx.send(VideoMessage::Unpause).await.unwrap();
            }
            ProtoMessage::Stop => {
                tx.send(VideoMessage::Pause).await.unwrap();
            }
            ProtoMessage::Media(link) => {
                tx.send(VideoMessage::Media(link)).await.unwrap();
            }
        }
    }
}

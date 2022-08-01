use argh::FromArgs;
use rand::Rng;

mod cli;
use cli::CliMode;

use tokio::sync::mpsc;
use uniplay::{MpvPlayer, ProtoMessage, State, VideoPlayer};

#[derive(FromArgs, Debug)]
/// Tool for syncing video playback
struct Args {
    /// autostart mpv
    #[argh(switch)]
    autostart: bool,

    /// server ip/domain
    #[argh(
        option,
        long = "server",
        default = r#"String::from("test.mosquitto.org")"#
    )]
    server: String,

    /// server port
    #[argh(option, long = "port", default = "1883")]
    port: u16,

    /// username
    #[argh(option, long = "name", default = "rnd_name()")]
    name: String,

    /// name of room
    #[argh(option, long = "room", default = r#"String::from("default_room")"#)]
    room: String,

    /// path to mpv socket
    #[argh(option, long = "ipc", default = r#"String::from("/tmp/mpv.sock")"#)]
    ipc_path: String,

    /// cli mode: 'repl' or 'spoof'
    #[argh(option, long = "cli", default = "CliMode::Repl")]
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

    let uni = uniplay::UniplayOpts {
        name: args.name.clone(),
        server: args.server,
        port: args.port,
        topic: topic.clone(),
    };

    let mpv_player = MpvPlayer {
        ipc_path: args.ipc_path,
    };
    if args.autostart {
        mpv_player.start();
    }
    let (player_sender, player_receiver) = mpsc::channel(8);
    let (event_sender, event_receiver) = mpsc::channel(8);
    let mpv_handle = tokio::spawn(mpv_player.run(player_receiver, Some(event_sender)));

    let (client, event_loop) = uni.spawn().await;
    State::spawn(event_loop, player_sender, event_receiver).await;
    tokio::spawn(async move {
        args.cli.run(client, &args.name, &topic).await;
    });

    mpv_handle.await.unwrap();
}

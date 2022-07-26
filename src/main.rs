use argh::FromArgs;
use rand::Rng;

mod cli;
use cli::CliMode;

use uniplay::{MpvPlayer, ProtoMessage, VideoPlayer};

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

    let uni = uniplay::UniplayOpts {
        name: args.name.clone(),
        server: args.server,
        port: args.port,
        topic: topic.clone(),
    };
    let (client, vd_rx) = uni.spawn().await;

    let mpv_player = MpvPlayer {
        ipc_path: args.ipc_path,
    };
    if args.autostart {
        mpv_player.start();
    }
    let mpv_handle = tokio::spawn(mpv_player.run(vd_rx));

    tokio::spawn(async move {
        args.cli.run(client, &args.name, &topic).await;
    });

    mpv_handle.await.unwrap();
}

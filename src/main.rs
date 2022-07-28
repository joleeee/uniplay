use argh::FromArgs;
use rand::Rng;

mod cli;
use cli::CliMode;

use tokio::sync::oneshot;
use uniplay::{MpvPlayer, ProtoMessage, VideoPlayer};

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
    let (client, vd_receiver) = uni.spawn().await;

    let mpv_player = MpvPlayer {
        ipc_path: args.ipc_path,
    };
    if args.autostart {
        mpv_player.start();
    }

    let (mpv_sender, mpv_receiver) = oneshot::channel();
    let mpv_handle = tokio::spawn(mpv_player.run(vd_receiver, mpv_sender));

    let cli_handle = tokio::spawn(async move {
        args.cli.run(client, &args.name, &topic).await;
    });

    tokio::select! {
        err = mpv_receiver => {
            let err = err.expect("recv error");
            println!("{}", err);
        }
        err = mpv_handle => {
            println!("mpv quit: {:?}", err);
        }
        err = cli_handle => {
            println!("cli quit: {:?}", err);
        }
    }

    // stop even when repl is blocking for input
    std::process::exit(0);
}

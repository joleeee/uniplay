use async_trait::async_trait;
use mpvi::{option, Mpv};
use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

use crate::{VideoMessage, VideoPlayer};

pub struct MpvPlayer {
    pub ipc_path: String,
}

#[async_trait]
impl VideoPlayer for MpvPlayer {
    type Error = std::io::Error;

    fn start(&self) -> std::process::Child {
        let ipc_arg = format!("{}={}", "--input-ipc-server", self.ipc_path);

        let c = Command::new("mpv")
            .arg(ipc_arg)
            .arg("--idle")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        // ew
        thread::sleep(Duration::from_millis(400));

        c
    }

    async fn run(
        self,
        mut receiver: mpsc::Receiver<VideoMessage>,
        os_sender: oneshot::Sender<Self::Error>,
    ) {
        let mpv = match Mpv::new(&self.ipc_path).await {
            Ok(mpv) => mpv,
            Err(e) => {
                println!("error connecting to mpv, is mpv running?");
                println!(
                    r#"start with "mpv --input-ipc-server={} --idle""#,
                    self.ipc_path
                );
                os_sender.send(e).expect("failed to send oneshot notif");
                return;
            }
        };

        loop {
            let msg = receiver.recv().await.expect("closed");
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

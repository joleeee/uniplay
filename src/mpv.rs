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

#[derive(thiserror::Error, Debug)]
pub enum MpvPlayerError {
    #[error(
        r#"failed to connect to mpv: `{0}`, is mpv running?
start with "mpv --input-ipc-server=<ipc_path> --idle""#
    )]
    ConnectionError(std::io::Error),
}

#[async_trait]
impl VideoPlayer for MpvPlayer {
    type Error = MpvPlayerError;

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
        event_sender: Option<mpsc::Sender<mpvi::Event>>,
    ) {
        let mpv = match Mpv::new(&self.ipc_path).await {
            Ok(mpv) => mpv,
            Err(e) => {
                os_sender.send(MpvPlayerError::ConnectionError(e)).unwrap();
                return;
            }
        };

        if let Some(sender) = event_sender {
            mpv.subscribe_events(sender)
                .await
                .expect("failed to subscribe");
        }

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

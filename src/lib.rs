use async_trait::async_trait;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

mod mpv;
pub use mpv::MpvPlayer;

mod state;
pub use state::State;

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
pub trait VideoPlayer {
    fn start(&self) -> std::process::Child;
    async fn run(
        self,
        receiver: mpsc::Receiver<VideoMessage>,
        event_sender: Option<mpsc::Sender<mpvi::Event>>,
    );
}

pub struct UniplayOpts {
    pub name: String,
    pub server: String,
    pub port: u16,

    pub topic: String,
}

impl UniplayOpts {
    pub async fn spawn(&self) -> (EventLoop, mpsc::Sender<ProtoMessage>) {
        let mut mqttoptions = MqttOptions::new(&self.name, &self.server, self.port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));
        let (client, eventloop) = AsyncClient::new(mqttoptions, 10);
        client
            .subscribe(&self.topic, QoS::ExactlyOnce)
            .await
            .unwrap();

        let (proto_sender, mut proto_receiver) = mpsc::channel::<ProtoMessage>(8);

        let topic = self.topic.clone();
        tokio::spawn(async move {
            loop {
                let msg = proto_receiver.recv().await.expect("closed");
                let txt = serde_json::to_string(&msg).unwrap();
                client
                    .publish(&topic, QoS::ExactlyOnce, false, txt.as_bytes())
                    .await
                    .expect("failed to send");
            }
        });

        proto_sender
            .send(ProtoMessage::Join(self.name.clone()))
            .await
            .expect("closed");

        (eventloop, proto_sender)
    }
}

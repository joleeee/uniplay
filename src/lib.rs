use async_trait::async_trait;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

mod mpv;
pub use mpv::MpvPlayer;

mod state;
use state::{mqtt_listen, relay};

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
    async fn run(self, rx: mpsc::Receiver<VideoMessage>);
}

pub struct UniplayOpts {
    pub name: String,
    pub server: String,
    pub port: u16,

    pub topic: String,
}

impl UniplayOpts {
    pub async fn spawn(&self) -> (AsyncClient, mpsc::Receiver<VideoMessage>) {
        let (pt_tx, pt_rx) = mpsc::channel::<ProtoMessage>(8);
        let (vd_tx, vd_rx) = mpsc::channel::<VideoMessage>(8);

        let mut mqttoptions = MqttOptions::new(&self.name, &self.server, self.port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));
        let (client, eventloop) = AsyncClient::new(mqttoptions, 10);
        client
            .subscribe(&self.topic, QoS::ExactlyOnce)
            .await
            .unwrap();

        let msg = serde_json::to_string(&ProtoMessage::Join(self.name.clone())).unwrap();
        client
            .publish(&self.topic, QoS::ExactlyOnce, false, msg.as_bytes())
            .await
            .unwrap();

        tokio::spawn(relay(pt_rx, vd_tx));

        tokio::spawn(mqtt_listen(eventloop, pt_tx));

        (client, vd_rx)
    }
}

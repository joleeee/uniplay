use rumqttc::{AsyncClient, QoS};
use std::time::Duration;
use strum::EnumString;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    time::sleep,
};

use crate::ProtoMessage;

#[derive(EnumString, Debug, Clone, Copy)]
pub enum CliMode {
    Repl,
    Spoof,
}

impl CliMode {
    pub async fn run(&self, client: AsyncClient, user: &String, topic: &String) {
        match self {
            Self::Repl => {
                repl(client, user, topic).await;
            }
            Self::Spoof => {
                spoof(client, topic).await;
            }
        }
    }
}

#[derive(EnumString)]
#[strum(serialize_all = "snake_case")]
enum ReplCmd {
    Set,
    Pause,
    Play,
    Chat,
}

async fn repl(client: AsyncClient, user: &String, topic: &String) {
    println!("commands: [set <link>, play <seconds>, pause]");

    let stdin = tokio::io::stdin();
    let stdin = BufReader::new(stdin);
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await.unwrap() {
        let (keyword, arg) = {
            let raw = line.trim();

            raw.split_once(' ').unwrap_or((raw, ""))
        };

        let keyword: ReplCmd = {
            let cmd = keyword.parse();
            if let Ok(cmd) = cmd {
                cmd
            } else {
                println!("repl: unknown command");
                continue;
            }
        };

        let msg = match keyword {
            ReplCmd::Set => ProtoMessage::Media(arg.to_string()),
            ReplCmd::Pause => ProtoMessage::Stop,
            ReplCmd::Play => ProtoMessage::PlayFrom(arg.parse().unwrap()),
            ReplCmd::Chat => ProtoMessage::Chat(user.clone(), arg.to_string()),
        };

        let msg = serde_json::to_string(&msg).unwrap();
        client
            .publish(topic, QoS::ExactlyOnce, false, msg)
            .await
            .unwrap();
    }
}

async fn spoof(client: AsyncClient, topic: &String) {
    let messages = vec![
        ProtoMessage::Media("https://youtu.be/jNQXAC9IVRw".to_string()),
        ProtoMessage::PlayFrom(2.0),
        ProtoMessage::Stop,
        ProtoMessage::PlayFrom(4.0),
    ];

    sleep(Duration::from_millis(500)).await;
    for msg in messages {
        let msg = serde_json::to_string(&msg).unwrap();

        client
            .publish(topic, QoS::ExactlyOnce, false, msg)
            .await
            .unwrap();

        sleep(Duration::from_millis(10_000)).await;
    }
}

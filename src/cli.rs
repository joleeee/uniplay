use rumqttc::{AsyncClient, QoS};
use std::{str::FromStr, time::Duration};
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

enum ReplCmd {
    Set(String),
    Pause,
    Play(f64),
    Chat(String),
}

#[derive(Debug)]
enum ReplParseError {
    NoSuchCommand,
    BadArgument,
}

impl FromStr for ReplCmd {
    type Err = ReplParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (keyword, arg) = s.split_once(' ').unwrap_or((s, ""));
        let arg = arg.trim();

        fn arg_to_string(arg: &str) -> Result<String, ReplParseError> {
            if arg.len() > 0 {
                Ok(arg.to_string())
            } else {
                Err(ReplParseError::BadArgument)
            }
        }

        match keyword {
            "set" => Ok(ReplCmd::Set(arg_to_string(arg)?)),
            "pause" => Ok(ReplCmd::Pause),
            "play" => match arg.parse() {
                Ok(dur) => Ok(ReplCmd::Play(dur)),
                Err(_) => Err(ReplParseError::BadArgument),
            },
            "chat" => Ok(ReplCmd::Chat(arg_to_string(arg)?)),
            _ => Err(ReplParseError::NoSuchCommand),
        }
    }
}

async fn repl(client: AsyncClient, user: &String, topic: &String) {
    println!("commands: [set <link>, play <seconds>, pause]");

    let stdin = tokio::io::stdin();
    let stdin = BufReader::new(stdin);
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await.unwrap() {
        let keyword: ReplCmd = {
            let cmd = line.parse();
            if let Ok(cmd) = cmd {
                cmd
            } else {
                println!("repl: unknown command: {:?}", cmd.err().unwrap());
                continue;
            }
        };

        let msg = match keyword {
            ReplCmd::Set(p) => ProtoMessage::Media(p),
            ReplCmd::Pause => ProtoMessage::Stop,
            ReplCmd::Play(d) => ProtoMessage::PlayFrom(d),
            ReplCmd::Chat(m) => ProtoMessage::Chat(user.clone(), m),
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

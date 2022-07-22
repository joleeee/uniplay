use rumqttc::{AsyncClient, QoS};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    time::sleep,
};

use crate::ProtoMessage;

pub async fn repl(client: AsyncClient, user: String, topic: &String) {
    println!("commands: [set <link>, play <seconds>, pause]");

    let stdin = tokio::io::stdin();
    let stdin = BufReader::new(stdin);
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await.unwrap() {
        let (keyword, arg) = {
            let raw = line.trim();

            raw.split_once(' ').unwrap_or((raw, ""))
        };

        let msg = match keyword {
            "set" => Some(ProtoMessage::Media(arg.to_string())),
            "pause" => Some(ProtoMessage::Stop),
            "play" => Some(ProtoMessage::PlayFrom(arg.parse().unwrap())),
            "chat" => Some(ProtoMessage::Chat(user.clone(), arg.to_string())),
            _ => {
                println!("unknown command");
                None
            }
        };

        if let Some(msg) = msg {
            let msg = serde_json::to_string(&msg).unwrap();
            client
                .publish(topic, QoS::ExactlyOnce, false, msg)
                .await
                .unwrap();
        }
    }
}

pub async fn spoof(client: AsyncClient, topic: &String) {
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

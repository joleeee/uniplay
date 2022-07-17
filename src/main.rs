use mpvipc::*;
use rumqttc::{Client, Connection, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::{io, sync::mpsc, thread, time::Duration};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Play(f64),
    Pause,
    Media(String),
}

fn main() {
    println!(r#"start mpv with "mpv --input-ipc-server=/tmp/mpv.sock --idle""#);

    let (tx, rx) = mpsc::channel::<Message>();
    let mpv_handle = thread::spawn(|| mpv(rx));

    let mut mqttoptions = MqttOptions::new("uniplay", "test.mosquitto.org", 1883);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (mut client, connection) = Client::new(mqttoptions, 10);
    client.subscribe("uniplay/thing", QoS::ExactlyOnce).unwrap();

    thread::spawn(|| {
        mqtt_listen(connection, tx);
    });

    thread::spawn(|| {
        if true {
            repl(client);
        } else {
            mqtt_spoof(client);
        }
    });

    mpv_handle.join().unwrap();
}

fn mqtt_listen(mut connection: Connection, tx: mpsc::Sender<Message>) {
    use rumqttc::{Event, Packet};
    for msg in connection
        .iter()
        .map(Result::unwrap)
        .filter_map(|notification| {
            if let Event::Incoming(v) = notification {
                Some(v)
            } else {
                None
            }
        })
        .filter_map(|incoming| {
            if let Packet::Publish(p) = incoming {
                Some(p)
            } else {
                None
            }
        })
        .map(|publish| {
            let msg: Message = serde_json::from_slice(&publish.payload).unwrap();
            msg
        })
    {
        tx.send(msg).unwrap();
    }
}

fn mpv(rx: mpsc::Receiver<Message>) {
    let mpv = Mpv::connect("/tmp/mpv.sock").unwrap();

    for msg in rx.iter() {
        println!("got {:?}", msg);
        match msg {
            Message::Play(pos) => {
                mpv.seek(pos, SeekOptions::Absolute).expect("play: failed to seek");
                mpv.set_property("pause", false).expect("play: failed to unpause");
            }
            Message::Pause => mpv.pause().expect("pause: failed to pause"),
            Message::Media(link) => {
                mpv.playlist_add(
                    &link,
                    PlaylistAddTypeOptions::File,
                    PlaylistAddOptions::Append,
                )
                .unwrap();
                let playlist = mpv.get_playlist().unwrap();

                let entry = playlist
                    .0
                    .iter()
                    .find(|entry| {
                        let a = entry.filename.as_str();
                        let b = link.as_str();
                        a == b
                    })
                    .unwrap();

                mpv.playlist_play_id(entry.id).unwrap();
                // but start off paused
                mpv.pause().unwrap();
            }
        }
    }
}

fn repl(mut client: Client) {
    println!("commands: [set <link>, play <seconds>, pause]");

    let stdin = io::stdin();
    let mut input = String::new();
    while stdin.read_line(&mut input).is_ok() {
        let (keyword, arg) = {
            let vec: Vec<_> = input.trim().split(' ').collect();
            (*vec.get(0).expect("no keyword"), *vec.get(1).unwrap_or(&""))
        };

        let msg = match keyword {
            "set" => Some(Message::Media(arg.to_string())),
            "pause" => Some(Message::Pause),
            "play" => Some(Message::Play(arg.parse().unwrap())),
            _ => {
                println!("unknown command");
                None
            }
        };

        if let Some(msg) = msg {
            let msg = serde_json::to_string(&msg).unwrap();
            client
                .publish("uniplay/thing", QoS::ExactlyOnce, false, msg)
                .unwrap();
        }

        input = String::new();
    }
}

fn mqtt_spoof(mut client: Client) {
    let messages = vec![
        Message::Media("https://youtu.be/jNQXAC9IVRw".to_string()),
        Message::Play(2.0),
        Message::Pause,
        Message::Play(4.0),
    ];

    thread::sleep(Duration::from_millis(500));
    for msg in messages {
        let msg = serde_json::to_string(&msg).unwrap();

        client
            .publish("uniplay/thing", QoS::ExactlyOnce, false, msg)
            .unwrap();

        thread::sleep(Duration::from_millis(10_000));
    }
}

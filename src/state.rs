use futures::FutureExt;
use rumqttc::EventLoop;
use tokio::sync::mpsc;

use crate::{ProtoMessage, VideoMessage};

pub struct State;

impl State {
    pub async fn spawn(
        eventloop: EventLoop,
        player_sender: mpsc::Sender<VideoMessage>,
        event_receiver: mpsc::Receiver<mpvi::Event>,
    ) {
        let (proto_sender, proto_receiver) = mpsc::channel::<ProtoMessage>(8);

        tokio::spawn(decode_incoming(eventloop, proto_sender));
        tokio::spawn(state_machine(proto_receiver, event_receiver, player_sender));
    }
}

async fn decode_incoming(mut eventloop: EventLoop, sender: mpsc::Sender<ProtoMessage>) {
    use rumqttc::{Event, Packet};

    fn decode_event(event: Event) -> Option<ProtoMessage> {
        let incoming = if let Event::Incoming(v) = event {
            Some(v)
        } else {
            None
        }?;

        let publish = if let Packet::Publish(p) = incoming {
            Some(p)
        } else {
            None
        }?;

        serde_json::from_slice(&publish.payload).unwrap_or_else(|e| {
            println!("deser error: {}", e);
            None
        })
    }

    loop {
        let msg = eventloop.poll().map(Result::unwrap).map(decode_event).await;

        if let Some(msg) = msg {
            sender.send(msg).await.unwrap();
        }
    }
}

async fn state_machine(
    mut proto_receiver: mpsc::Receiver<ProtoMessage>,
    mut event_receiver: mpsc::Receiver<mpvi::Event>,
    sender: mpsc::Sender<VideoMessage>,
) {
    loop {
        tokio::select! {
            msg = proto_receiver.recv() => {
                execute_protomsg(msg.expect("closed"), &sender).await;
            },
            event = event_receiver.recv() => {
                let event = event.expect("closed");
                handle_event(event).await;
            }
        }
    }
}

async fn handle_event(event: mpvi::Event) {
    use mpvi::Event;
    match event {
        Event::Pause => println!("user paused!"),
        Event::Unpause => println!("user unpaused!"),
        _ => {}
    }
}

async fn execute_protomsg(msg: ProtoMessage, sender: &mpsc::Sender<VideoMessage>) {
    match msg {
        ProtoMessage::Join(name) => {
            println!("{} joined the room.", name);
        }
        ProtoMessage::Chat(from, msg) => {
            println!("<{}> {}", from, msg);
        }
        ProtoMessage::PlayFrom(pos) => {
            sender.send(VideoMessage::Seek(pos)).await.unwrap();
            sender.send(VideoMessage::Unpause).await.unwrap();
        }
        ProtoMessage::Stop => {
            sender.send(VideoMessage::Pause).await.unwrap();
        }
        ProtoMessage::Media(link) => {
            sender.send(VideoMessage::Media(link)).await.unwrap();
        }
    }
}

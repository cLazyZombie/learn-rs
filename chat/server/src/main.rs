use std::{collections::HashMap, net::SocketAddr, thread::sleep, time::Duration};

use libs::Packet;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
};

#[allow(dead_code)]
#[derive(Debug)]
enum Message {
    Packet((SocketAddr, Packet)),
    Connected(ConnectionInfo),
    Disconnected(SocketAddr),
    Quit,
}

#[derive(Debug)]
struct ConnectionInfo {
    pub addr: SocketAddr,
    pub packet_req_sender: tokio::sync::mpsc::UnboundedSender<Packet>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("localhost:6767").await?;
    let (message_send_ch, message_recv_ch) = crossbeam_channel::unbounded::<Message>();

    let _thread_handle = std::thread::spawn(move || message_handler(message_recv_ch));

    loop {
        let (incoming, addr) = listener.accept().await?;
        let (reader, writer) = incoming.into_split();

        let (packet_req_sender, packet_req_recver) = tokio::sync::mpsc::unbounded_channel();

        let connection_message = ConnectionInfo {
            addr,
            packet_req_sender,
        };
        message_send_ch
            .send(Message::Connected(connection_message))
            .unwrap();

        tokio::spawn(packet_recver(addr, reader, message_send_ch.clone()));
        tokio::spawn(packet_sender(
            addr,
            writer,
            packet_req_recver,
            message_send_ch.clone(),
        ));
    }
}

fn message_handler(recv_ch: crossbeam_channel::Receiver<Message>) {
    // let mut users = HashMap::<SocketAddr, tokio::sync::mpsc::UnboundedSender<Packet>>::new();
    let mut users = HashMap::new();

    'main_loop: loop {
        while let Ok(message) = recv_ch.try_recv() {
            match message {
                Message::Connected(info) => {
                    users.insert(info.addr, info.packet_req_sender);
                    eprintln!(
                        "[user count: {}] user connected; {:?}",
                        users.len(),
                        info.addr
                    );
                }
                Message::Packet((sender_addr, packet)) => {
                    eprintln!("packet to send: {:?}", packet);

                    // send packet to users
                    for (addr, packet_req_sender) in &mut users {
                        // don't send packet to sender
                        if *addr == sender_addr {
                            continue;
                        }

                        let _result = packet_req_sender.send(packet.clone());
                    }
                }
                Message::Disconnected(addr) => {
                    users.remove(&addr);
                    eprintln!(
                        "[user count: {}] user disconnected; {:?}",
                        users.len(),
                        addr
                    );
                }
                Message::Quit => break 'main_loop,
            }
        }

        sleep(Duration::from_secs_f32(0.1));
    }
}

const PACKET_BUFFER_SIZE: usize = 1024 * 1024;

async fn packet_recver<F: AsyncRead + Unpin>(
    addr: SocketAddr,
    mut socket: F,
    message_send_ch: crossbeam_channel::Sender<Message>,
) {
    eprintln!("start packet recver {:?}", addr);

    let mut buffer = vec![0_u8; PACKET_BUFFER_SIZE];
    loop {
        // read pakcet size
        let size = match socket.read_u16().await {
            Ok(size) => size as usize,
            Err(_) => {
                let _ = message_send_ch.send(Message::Disconnected(addr));
                break;
            }
        };

        if size > PACKET_BUFFER_SIZE || size == 0 {
            break;
        }

        eprintln!("read size: {}", size);
        buffer.resize(size, 0);

        // read body
        let buf: &mut [u8] = &mut buffer;
        let success = match socket.read_exact(buf).await {
            Ok(written_size) if written_size != size => false,
            Err(_) => false,
            Ok(_) => true,
        };

        if !success {
            let _ = message_send_ch.send(Message::Disconnected(addr));
            break;
        }

        // make packet and send to packet queue
        let packet = Packet::from_bytes(buf).unwrap();
        let msg = Message::Packet((addr, packet));
        message_send_ch.send(msg).unwrap();
    }

    eprintln!("end packet recver {:?}", addr);
}

async fn packet_sender<F: AsyncWrite + Unpin>(
    addr: SocketAddr,
    mut socket: F,
    mut request_packet_recver: tokio::sync::mpsc::UnboundedReceiver<Packet>,
    message_chan: crossbeam_channel::Sender<Message>,
) {
    eprintln!("start packet sender {:?}", addr);

    let mut buffer = Vec::with_capacity(PACKET_BUFFER_SIZE);
    loop {
        // get requested packet
        let packet = match request_packet_recver.recv().await {
            Some(packet) => packet,
            None => {
                eprintln!(
                    "request_packet_recver is closed by opposite channel. {:?}",
                    addr
                );
                break;
            }
        };

        // packet to buffer
        packet.fill_buffer(&mut buffer);

        // send buffer to stream
        let result = socket.write_all(&buffer).await;
        if result.is_err() {
            eprintln!("write to stream error. {:?}", addr);
            let _ = message_chan.send(Message::Disconnected(addr));
            break;
        }
    }

    eprintln!("end packet sender {:?}", addr);
}

#[cfg(test)]
mod tests {
    use std::{
        net::{Ipv4Addr, SocketAddrV4},
        time::Duration,
    };

    use byteorder::{BigEndian, ByteOrder};
    use libs::ChatPacket;

    use super::*;

    #[tokio::test]
    async fn recv_packet() {
        let (send_ch, recv_ch) = crossbeam_channel::unbounded();
        let packet = Packet::Chat(ChatPacket {
            name: "a".to_string(),
            message: "hello".to_string(),
        });
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8000));
        let mut packet_buffer = vec![0_u8; 2];
        let mut chat_packet_bytes = packet.get_bytes();
        let chat_packet_size = chat_packet_bytes.len() as u16;
        packet_buffer.append(&mut chat_packet_bytes);
        BigEndian::write_u16(&mut packet_buffer, chat_packet_size);
        let stream_reader: &[u8] = &packet_buffer;

        let _ = packet_recver(addr, stream_reader, send_ch).await;

        let packet = recv_ch.recv_timeout(Duration::from_secs(10)).unwrap();
        let (sender_addr, chat) = match packet {
            Message::Packet((addr, Packet::Chat(chat_packet))) => (addr, chat_packet),
            _ => panic!("packet should be chat"),
        };

        assert_eq!(sender_addr, addr);

        assert_eq!(
            chat,
            ChatPacket {
                name: "a".to_string(),
                message: "hello".to_string(),
            }
        );
    }
}

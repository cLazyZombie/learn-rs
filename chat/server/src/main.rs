use std::{collections::HashMap, net::SocketAddr, thread::sleep, time::Duration};

use libs::{ChatNotifyPacket, LoginConfirmPacket, LoginNotifyPacket, LogoutNotifyPacket, Packet};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc::UnboundedSender,
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
    pub name: String,
    pub packet_req_sender: tokio::sync::mpsc::UnboundedSender<Packet>,
}

#[derive(Debug)]
enum Error {
    IoError(std::io::Error),
    PacketSizeError(usize),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
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

        tokio::spawn(packet_recver(
            addr,
            reader,
            message_send_ch.clone(),
            packet_req_sender,
        ));

        tokio::spawn(packet_sender(
            addr,
            writer,
            packet_req_recver,
            message_send_ch.clone(),
        ));
    }
}

struct User {
    pub send_chan: UnboundedSender<Packet>,
    pub name: String,
}

fn message_handler(recv_ch: crossbeam_channel::Receiver<Message>) {
    // let mut users = HashMap::<SocketAddr, tokio::sync::mpsc::UnboundedSender<Packet>>::new();
    let mut users: HashMap<SocketAddr, User> = HashMap::new();

    'main_loop: loop {
        while let Ok(message) = recv_ch.try_recv() {
            match message {
                Message::Connected(info) => {
                    let user = User {
                        send_chan: info.packet_req_sender,
                        name: info.name.clone(),
                    };

                    // send login confirm
                    let login_confirm_packet = Packet::LoginConfirm(LoginConfirmPacket {
                        users: users.iter().map(|(_, user)| user.name.clone()).collect(),
                    });
                    if user.send_chan.send(login_confirm_packet).is_err() {
                        eprintln!("failed to send packet to user {}", user.name);
                    }

                    // add to users
                    users.insert(info.addr, user);
                    eprintln!(
                        "[user count: {}] user {} connected; {:?}",
                        users.len(),
                        info.name,
                        info.addr
                    );

                    // send login notify
                    let login_notify_packet = Packet::LoginNotify(LoginNotifyPacket {
                        name: info.name.clone(),
                    });
                    for (addr, user) in &users {
                        if *addr == info.addr {
                            continue;
                        }

                        if user.send_chan.send(login_notify_packet.clone()).is_err() {
                            eprintln!("failed to send packet to user {}", user.name);
                        }
                    }
                }
                Message::Packet((sender_addr, packet)) => {
                    eprintln!("received packet: {:?}", packet);

                    match packet {
                        Packet::Chat(chat_packet) => {
                            if let Some(sender) = users.get(&sender_addr) {
                                let chat_notify_packet = Packet::ChatNotify(ChatNotifyPacket {
                                    name: sender.name.clone(),
                                    message: chat_packet.message,
                                });

                                for (addr, user) in &users {
                                    // dont send to sender
                                    if *addr == sender_addr {
                                        continue;
                                    }

                                    if user.send_chan.send(chat_notify_packet.clone()).is_err() {
                                        eprintln!("failed to send packet to user {}", user.name);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Message::Disconnected(addr) => {
                    if let Some(logout_user) = users.remove(&addr) {
                        eprintln!(
                            "[user count: {}] user {} disconnected; {:?}",
                            users.len(),
                            logout_user.name,
                            addr
                        );
                        let logout_packet = Packet::LogoutNotify(LogoutNotifyPacket {
                            name: logout_user.name,
                        });
                        for user in users.values() {
                            if user.send_chan.send(logout_packet.clone()).is_err() {
                                eprintln!("failed to send packet to user {}", user.name);
                            }
                        }
                    }
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
    packet_req_sender: UnboundedSender<Packet>,
) {
    eprintln!("start packet recver {:?}", addr);

    let mut buffer = vec![0_u8; PACKET_BUFFER_SIZE];

    // read login packet
    let packet = read_packet(&mut socket, &mut buffer).await;
    if let Err(e) = packet {
        eprintln!("login packet error: {:?}", e);
        message_send_ch.send(Message::Disconnected(addr)).unwrap();
        return;
    }

    let packet = packet.unwrap();
    match packet {
        Packet::LoginReq(login_packet) => {
            let connection_message = ConnectionInfo {
                addr,
                name: login_packet.name,
                packet_req_sender,
            };
            message_send_ch
                .send(Message::Connected(connection_message))
                .unwrap();
        }
        _ => {
            eprintln!("login packet is expected, but {:?}", packet);
            return;
        }
    }

    // read packes
    loop {
        let read_result = read_packet(&mut socket, &mut buffer).await;
        match read_result {
            Ok(packet) => {
                message_send_ch
                    .send(Message::Packet((addr, packet)))
                    .unwrap();
            }
            Err(err) => {
                eprintln!("packet recver error: {:?}", err);
                let _ = message_send_ch.send(Message::Disconnected(addr));
                break;
            }
        }
    }

    eprintln!("end packet recver {:?}", addr);
}

async fn read_packet<F: AsyncRead + Unpin>(
    socket: &mut F,
    buffer: &mut Vec<u8>,
) -> Result<Packet, Error> {
    // read pakcet size
    let size = socket.read_u16().await? as usize;

    if size > PACKET_BUFFER_SIZE || size == 0 {
        return Err(Error::PacketSizeError(size));
    }

    buffer.resize(size, 0);

    // read body
    let buf: &mut [u8] = buffer;
    let body_size = socket.read_exact(buf).await?;

    if body_size != size {
        return Err(Error::PacketSizeError(body_size));
    }
    // let _ = message_send_ch.send(Message::Disconnected(addr));

    // make packet and send to packet queue
    let packet = Packet::from_bytes(buf).unwrap();
    Ok(packet)
}

async fn write_packet<F: AsyncWrite + Unpin>(
    socket: &mut F,
    packet: &Packet,
    buffer: &mut Vec<u8>,
) -> Result<(), Error> {
    packet.fill_buffer(buffer);
    socket.write_all(&buffer).await?;

    Ok(())
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

        let result = write_packet(&mut socket, &packet, &mut buffer).await;
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

    use libs::{ChatPacket, LoginReqPacket};
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    #[tokio::test]
    async fn recv_packet() {
        let (packet_req_sender, _packet_req_recver) = unbounded_channel();
        let (send_ch, recv_ch) = crossbeam_channel::unbounded();
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8000));

        let login_packet = Packet::LoginReq(LoginReqPacket {
            name: "foo".to_string(),
        });
        let login_buffer = make_packet_buffer(&login_packet);

        let chat_packet = Packet::Chat(ChatPacket {
            message: "hello".to_string(),
        });
        let chat_buffer = make_packet_buffer(&chat_packet);

        let mut buffer = Vec::new();
        buffer.extend(login_buffer);
        buffer.extend(chat_buffer);
        let async_buffer = tokio::io::BufReader::new(&buffer as &[u8]);
        let _ = packet_recver(addr, async_buffer, send_ch, packet_req_sender).await;

        // receive login
        let message = recv_ch.recv_timeout(Duration::from_secs(10)).unwrap();
        let (sender_addr, name) = match message {
            Message::Connected(connection_info) => (connection_info.addr, connection_info.name),
            _ => panic!("message should be connected, but {message:?}"),
        };

        assert_eq!(sender_addr, addr);
        assert_eq!(name, "foo");

        // receive chat
        let packet = recv_ch.recv_timeout(Duration::from_secs(10)).unwrap();
        let (sender_addr, chat) = match packet {
            Message::Packet((addr, Packet::Chat(chat_packet))) => (addr, chat_packet),
            _ => panic!("packet should be chat, but {packet:?}"),
        };

        assert_eq!(sender_addr, addr);

        assert_eq!(
            chat,
            ChatPacket {
                message: "hello".to_string(),
            }
        );
    }

    fn make_packet_buffer(packet: &Packet) -> Vec<u8> {
        // let mut packet_buffer = vec![0_u8; 2];
        // let mut chat_packet_bytes = packet.get_bytes();
        // let chat_packet_size = chat_packet_bytes.len() as u16;
        // packet_buffer.append(&mut chat_packet_bytes);
        // BigEndian::write_u16(&mut packet_buffer, chat_packet_size);
        // packet_buffer

        let mut buffer = Vec::new();
        packet.fill_buffer(&mut buffer);
        buffer
    }
}

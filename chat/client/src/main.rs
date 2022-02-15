use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::mpsc,
};

use byteorder::{BigEndian, ByteOrder};
use libs::{ChatPacket, LoginReqPacket, Packet};

fn main() {
    // get name
    let name;
    {
        print!("Enter your name: ");
        std::io::stdout().flush().unwrap();
        let mut name_buf = String::new();
        std::io::stdin().read_line(&mut name_buf).unwrap();
        name = name_buf.trim().to_string();
    }

    let (packet_sender, packet_recver) = mpsc::channel::<Packet>();

    let mut stream = TcpStream::connect("localhost:6767").unwrap();
    let receive_stream = stream.try_clone().unwrap();

    let _h1 = std::thread::spawn(move || recv(receive_stream, packet_sender));
    let _h2 = std::thread::spawn(move || process_packet(packet_recver));

    // login
    let login_packet = Packet::LoginReq(LoginReqPacket { name });

    let mut buf = Vec::new();
    login_packet.fill_buffer(&mut buf);

    let result = stream.write_all(&buf);
    if let Err(error) = result {
        eprintln!("send error. {:?}", error);
    }

    // send chat
    let mut buf = String::new();
    while let Ok(_size) = std::io::stdin().read_line(&mut buf) {
        if buf.starts_with("quit") {
            break;
        }

        while buf.ends_with("\n") {
            buf.pop();
        }

        if buf.len() > 0 {
            let packet = Packet::Chat(ChatPacket {
                message: buf.clone(),
            });
            send_packet(&mut stream, packet);
        }
        buf.clear();
    }
}

const RECV_BUFFER_SIZE: usize = 1024 * 1024;

fn send_packet(stream: &mut TcpStream, packet: Packet) {
    let mut buf = Vec::new();
    packet.fill_buffer(&mut buf);
    let result = stream.write_all(&buf);
    if let Err(error) = result {
        eprintln!("send error. {:?}", error);
    }
}

fn recv(mut stream: TcpStream, packet_ch: mpsc::Sender<Packet>) {
    let mut header = [0_u8; 2];
    let mut packet_buffer = [0_u8; RECV_BUFFER_SIZE];

    loop {
        if let Err(err) = stream.read_exact(&mut header) {
            eprintln!("recv error when read header. {:?}", err);
            break;
        }

        let size = BigEndian::read_u16(&header) as usize;
        // eprintln!("packet size: {}", size);

        if let Err(err) = stream.read_exact(&mut packet_buffer[..size]) {
            eprintln!("recv error when read body. {:?}", err);
            break;
        }

        // eprintln!("contents: {:?}", &packet_buffer[..size]);

        match Packet::from_bytes(&packet_buffer[..size]) {
            Ok(packet) => packet_ch.send(packet).unwrap(),
            Err(err) => {
                eprintln!("error convert buffer to packet. err: {:?}", err);
                break;
            }
        }
    }
}

fn process_packet(packet_ch: mpsc::Receiver<Packet>) {
    loop {
        let recv = packet_ch.recv();
        match recv {
            Ok(Packet::LoginConfirm(login_confirm)) => {
                println!(
                    "login success. users: {}",
                    login_confirm
                        .users
                        .iter()
                        .map(|n| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(Packet::LoginNotify(login_notify)) => {
                println!("[User {} joined]", login_notify.name);
            }
            Ok(Packet::LogoutNotify(logout_notify)) => {
                println!("[User {} left]", logout_notify.name);
            }
            Ok(Packet::ChatNotify(chat_notify)) => {
                println!("[{}] {}", chat_notify.name, chat_notify.message);
            }
            Ok(packet) => {
                eprintln!("unknown packet: {:?}", packet);
            }
            Err(_) => break, // Err(err) => println!("error. {:?}", err),
        }
    }
}

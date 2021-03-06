use byteorder::{BigEndian, ByteOrder};
use serde::{Deserialize, Serialize};
use serde_json::Result;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Packet {
    LoginReq(LoginReqPacket),         // client -> server
    LoginConfirm(LoginConfirmPacket), // server -> client
    LoginNotify(LoginNotifyPacket),   // server -> client
    LogoutNotify(LogoutNotifyPacket), // server -> client
    Chat(ChatPacket),                 // client to server
    ChatNotify(ChatNotifyPacket),     // server to client
}

impl Packet {
    const HEADER_LEN: usize = 2;

    pub fn get_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Packet> {
        serde_json::from_slice(buf)
    }

    pub fn fill_buffer<'a>(&self, mut buf: &'a mut Vec<u8>) {
        buf.resize(Self::HEADER_LEN, 0);
        serde_json::to_writer(&mut buf, self).unwrap();

        let contents_len = (buf.len() - Self::HEADER_LEN) as u16;
        BigEndian::write_u16(&mut buf, contents_len);
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ChatPacket {
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ChatNotifyPacket {
    pub name: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct LoginReqPacket {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct LoginConfirmPacket {
    pub users: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct LoginNotifyPacket {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct LogoutNotifyPacket {
    pub name: String,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_packet() {
        let packet = Packet::Chat(ChatPacket {
            message: "hello".to_string(),
        });
        let bytes = packet.get_bytes();

        let deserialized = Packet::from_bytes(&bytes).unwrap();

        assert_eq!(packet, deserialized);
    }
}

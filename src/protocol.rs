use aes_gcm::{AeadInPlace, Aes128Gcm, KeyInit, Nonce, Tag};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use tokio::net::UdpSocket;

const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

const KEY: &[u8] = b"X3V.bXCfe3EhN'wb";
const ASSOCIATED_DATA: &[u8] = &[
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
];

#[derive(Debug)]
pub enum ProtocolError {
    Encrypt(aes_gcm::Error),
    Io(std::io::Error),
}

impl Error for ProtocolError {}

impl Display for ProtocolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::Encrypt(err) => write!(f, "{}", err),
            ProtocolError::Io(err) => write!(f, "{}", err),
        }
    }
}

impl From<aes_gcm::Error> for ProtocolError {
    fn from(value: aes_gcm::Error) -> Self {
        ProtocolError::Encrypt(value)
    }
}

impl From<std::io::Error> for ProtocolError {
    fn from(value: std::io::Error) -> Self {
        ProtocolError::Io(value)
    }
}

fn encrypt_packet(mut data: Vec<u8>) -> Result<Box<[u8]>, aes_gcm::Error> {
    let cipher = Aes128Gcm::new_from_slice(KEY).unwrap();

    let nonce_bytes: [u8; NONCE_BYTES] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let tag = cipher.encrypt_in_place_detached(nonce, ASSOCIATED_DATA, &mut data)?;

    // adjust `data` to be [nonce][tag][data]
    let prefix_iter = nonce_bytes.into_iter().chain(tag);
    data.splice(0..0, prefix_iter);

    Ok(data.into_boxed_slice())
}

fn decrypt_packet(packet: &mut [u8]) -> Result<&mut [u8], aes_gcm::Error> {
    let (nonce_bytes, after_nonce) = packet.split_at_mut(NONCE_BYTES);
    let (tag_bytes, data) = after_nonce.split_at_mut(TAG_BYTES);

    let cipher = Aes128Gcm::new_from_slice(KEY).unwrap();

    let nonce = Nonce::from_slice(&nonce_bytes);
    let tag = Tag::from_slice(&tag_bytes);

    cipher.decrypt_in_place_detached(nonce, ASSOCIATED_DATA, data, tag)?;
    Ok(data)
}

pub async fn send_connect(socket: &UdpSocket, user_id: u64) -> Result<(), ProtocolError> {
    let mut connect_data = Vec::new();
    connect_data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    connect_data.extend_from_slice(b"Hconnect\0");
    connect_data.extend_from_slice(&user_id.to_le_bytes());
    connect_data.push(2);

    let encrypted_data = encrypt_packet(connect_data)?;

    let mut cursor = 0;
    while cursor < encrypted_data.len() {
        cursor += socket.send(&encrypted_data[cursor..]).await?;
    }
    Ok(())
}

pub async fn receive_connect_reply(socket: &UdpSocket, user_id: u64) -> Result<(), ProtocolError> {
    let mut buffer = vec![0; 1500];
    loop {
        let read_len = socket.recv(&mut buffer).await?;
        if read_len == 0 {
            return Err(tokio::io::Error::from(tokio::io::ErrorKind::UnexpectedEof).into());
        }

        let data = decrypt_packet(&mut buffer[..read_len])?;

        // 0-4: i32 = -1
        // 4-5: u8  = 'I'
        // 5-9: i32 = challenge
        // 9-17: u64 = uid
        // 17-25: str = "connect\0"
        // 25-29: ?

        if data.len() < 29 {
            continue;
        };

        let field_0 = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if field_0 != -1 {
            continue;
        };

        let field_1 = data[4];
        if field_1 != 0x49 {
            continue;
        };

        let field_3 = u64::from_le_bytes([
            data[17], data[18], data[19], data[20], data[21], data[22], data[23], data[24],
        ]);
        if field_3 != user_id {
            continue;
        };

        let field_4 = &data[17..25];
        if field_4 != b"connect\0" {
            continue;
        };

        return Ok(());
    }
}

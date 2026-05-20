use std::io::{self, Read, Write};

pub const HEADER_SIZE: usize = 8;
const MAGIC: &[u8; 2] = b"HE";
const MAX_PAYLOAD_SIZE: u32 = 20 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketType {
    Name = 0,
    Message = 1,
    Picture = 2,
}

impl TryFrom<u16> for PacketType {
    type Error = io::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Name),
            1 => Ok(Self::Message),
            2 => Ok(Self::Picture),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown packet type {value}"),
            )),
        }
    }
}

pub struct Packet {
    pub packet_type: PacketType,
    pub payload: Vec<u8>,
}

pub fn write_packet<W: Write>(
    writer: &mut W,
    packet_type: PacketType,
    payload: &[u8],
) -> io::Result<()> {
    let payload_len: u32 = payload
        .len()
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "payload too large"))?;

    let mut header = Vec::with_capacity(HEADER_SIZE);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&(packet_type as u16).to_be_bytes());
    header.extend_from_slice(&payload_len.to_be_bytes());

    writer.write_all(&header)?;
    writer.write_all(payload)?;
    writer.flush()
}

pub fn read_packet<R: Read>(reader: &mut R) -> io::Result<Packet> {
    let mut header = [0_u8; HEADER_SIZE];
    reader.read_exact(&mut header)?;

    if &header[0..2] != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid packet magic",
        ));
    }

    let packet_type = PacketType::try_from(u16::from_be_bytes([header[2], header[3]]))?;
    let payload_len = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);

    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "payload exceeds maximum size",
        ));
    }

    let mut payload = vec![0_u8; payload_len as usize];
    reader.read_exact(&mut payload)?;

    Ok(Packet {
        packet_type,
        payload,
    })
}

pub fn identity_payload(student_name: &str, student_id: &str) -> Vec<u8> {
    format!("{student_name}|{student_id}").into_bytes()
}

pub fn parse_identity(payload: &[u8]) -> (String, String) {
    let value = String::from_utf8_lossy(payload);
    let mut parts = value.splitn(2, '|');
    let name = parts.next().unwrap_or("Unknown").to_string();
    let student_id = parts.next().unwrap_or("").to_string();
    (name, student_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn packet_round_trip_matches_swift_header_format() {
        let mut bytes = Vec::new();
        write_packet(&mut bytes, PacketType::Name, b"Ada|STU-001").unwrap();

        assert_eq!(&bytes[0..2], b"HE");
        assert_eq!(u16::from_be_bytes([bytes[2], bytes[3]]), 0);
        assert_eq!(
            u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            11
        );

        let packet = read_packet(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(packet.packet_type, PacketType::Name);
        assert_eq!(packet.payload, b"Ada|STU-001");
    }

    #[test]
    fn identity_parser_keeps_legacy_name_only_payloads() {
        assert_eq!(
            parse_identity(b"Ada Lovelace"),
            (String::from("Ada Lovelace"), String::new())
        );
    }
}

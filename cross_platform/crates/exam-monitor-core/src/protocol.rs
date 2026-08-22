use std::io::{self, Read, Write};

pub const HEADER_SIZE: usize = 8;

/// Sent by the server as a `Message` packet just before it closes a
/// connection that presented the wrong class code. Without it the client can
/// only report a raw socket error, which looks like a network fault.
pub const REJECT_WRONG_CODE: &str = "WRONG_CODE";

/// Sent by the client as a `Message` packet when the OS refuses screen
/// capture. Without it a blocked student looks perfectly monitored while the
/// proctor is really watching an empty desktop.
pub const CLIENT_NO_SCREEN_PERMISSION: &str = "NO_SCREEN_PERMISSION";

/// Prefix marking a generated per-install device id rather than a real
/// student ID, so the server can dedup on it without displaying it.
pub const DEVICE_ID_PREFIX: &str = "dev:";
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

    // Grow the buffer as bytes actually arrive rather than trusting the
    // claimed length up front — a stalled connection claiming 20MB would
    // otherwise pin 20MB of RAM while sending nothing.
    let mut payload = Vec::with_capacity((payload_len as usize).min(64 * 1024));
    reader
        .by_ref()
        .take(u64::from(payload_len))
        .read_to_end(&mut payload)?;
    if payload.len() != payload_len as usize {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed mid-payload",
        ));
    }

    Ok(Packet {
        packet_type,
        payload,
    })
}

/// Identity payload is `joinCode|name|studentID`. The join code lets the
/// server reject connections that don't know the room's code. `|` is the field
/// separator, so it is stripped from each field to keep parsing unambiguous
/// (e.g. a student named "Smith | Jr").
pub fn identity_payload(join_code: &str, student_name: &str, student_id: &str) -> Vec<u8> {
    let clean = |value: &str| value.replace('|', " ");
    format!(
        "{}|{}|{}",
        clean(join_code),
        clean(student_name),
        clean(student_id)
    )
    .into_bytes()
}

pub fn parse_identity(payload: &[u8]) -> (String, String, String) {
    let value = String::from_utf8_lossy(payload);
    let mut parts = value.splitn(3, '|');
    let join_code = parts.next().unwrap_or("").to_string();
    let name = parts.next().unwrap_or("Unknown").to_string();
    let student_id = parts.next().unwrap_or("").to_string();
    (join_code, name, student_id)
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
    fn identity_round_trip_carries_join_code() {
        let payload = identity_payload("7GK4", "Ada Lovelace", "STU-001");
        assert_eq!(
            parse_identity(&payload),
            (
                String::from("7GK4"),
                String::from("Ada Lovelace"),
                String::from("STU-001"),
            )
        );
    }

    #[test]
    fn identity_strips_pipes_so_names_cannot_shift_fields() {
        // A name containing the field separator must not corrupt the parse.
        let payload = identity_payload("7GK4", "Smith | Jr", "STU-2");
        let (code, name, id) = parse_identity(&payload);
        assert_eq!(code, "7GK4");
        assert_eq!(name, "Smith   Jr");
        assert_eq!(id, "STU-2");
    }

    #[test]
    fn identity_parser_tolerates_missing_fields() {
        // A payload with only a code (no name/id) still parses without panic.
        assert_eq!(
            parse_identity(b"7GK4"),
            (String::from("7GK4"), String::from("Unknown"), String::new())
        );
    }
}

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct DecodedUdpEvent {
    pub event_name: String,
    pub payload_len: u32,
}

pub fn decode_udp_event(datagram: &[u8]) -> Result<DecodedUdpEvent> {
    if datagram.len() < 8 {
        return Err(anyhow!("datagram too short"));
    }
    let name_len = u32::from_be_bytes(
        datagram[0..4]
            .try_into()
            .map_err(|_| anyhow!("invalid name_len"))?,
    ) as usize;

    let after_name = 4 + name_len;
    if datagram.len() < after_name + 4 {
        return Err(anyhow!("datagram missing payload length"));
    }

    let name_bytes = &datagram[4..after_name];
    let payload_len = u32::from_be_bytes(
        datagram[after_name..after_name + 4]
            .try_into()
            .map_err(|_| anyhow!("invalid payload_len"))?,
    );

    let expected = after_name + 4 + payload_len as usize;
    if datagram.len() != expected {
        return Err(anyhow!("datagram length mismatch"));
    }

    let event_name = String::from_utf8_lossy(name_bytes).to_string();
    Ok(DecodedUdpEvent {
        event_name,
        payload_len,
    })
}

#[must_use]
pub fn encode_udp_event(event_name: &str, payload: &[u8]) -> Vec<u8> {
    let name_bytes = event_name.as_bytes();
    let mut out = Vec::with_capacity(8 + name_bytes.len() + payload.len());
    out.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(name_bytes);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::{decode_udp_event, encode_udp_event};

    #[test]
    fn round_trip_wire() {
        let event = "TEST_EVENT";
        let payload = b"abc123";
        let datagram = encode_udp_event(event, payload);
        let decoded = decode_udp_event(&datagram).expect("decode should succeed");
        assert_eq!(decoded.event_name, event);
        assert_eq!(decoded.payload_len, payload.len() as u32);
    }
}

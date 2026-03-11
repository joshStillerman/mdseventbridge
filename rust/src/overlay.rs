use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub const TOPIC_PREFIX: &str = "mdsevent.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayMeta {
    pub id: String,
    pub origin: String,
    pub sender: String,
    pub sender_udp_port: u16,
    pub hops: u32,
    pub event: String,
    pub ts: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

pub fn publish(pub_sock: &zmq::Socket, meta: &OverlayMeta, datagram: &[u8]) -> Result<()> {
    let topic = format!("{TOPIC_PREFIX}{}", meta.event).into_bytes();
    let meta_raw = serde_json::to_vec(meta)?;
    pub_sock.send_multipart([topic, meta_raw, datagram.to_vec()], 0)?;
    Ok(())
}

pub fn try_recv(sub_sock: &zmq::Socket) -> Result<Option<(OverlayMeta, Vec<u8>)>> {
    let frames = match sub_sock.recv_multipart(zmq::DONTWAIT) {
        Ok(f) => f,
        Err(zmq::Error::EAGAIN) => return Ok(None),
        Err(err) => return Err(anyhow!("overlay recv failed: {err}")),
    };

    if frames.len() != 3 {
        return Err(anyhow!("expected 3 frames, got {}", frames.len()));
    }

    let meta: OverlayMeta = serde_json::from_slice(&frames[1])?;
    Ok(Some((meta, frames[2].clone())))
}

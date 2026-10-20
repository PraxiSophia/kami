use crate::{wire::WireFrame, ProtocolError, StructurallyValidFrame};
use prost::Message;

pub const MAX_ENCODED_FRAME_SIZE: usize = 65_536;

pub fn decode_frame(bytes: &[u8]) -> Result<StructurallyValidFrame, ProtocolError> {
    if bytes.len() > MAX_ENCODED_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge);
    }
    crate::scan::scan(bytes)?;
    let wire = WireFrame::decode(bytes).map_err(|_| ProtocolError::MalformedWire)?;
    Ok(StructurallyValidFrame(wire))
}
pub fn encode_frame(frame: &StructurallyValidFrame) -> Result<Vec<u8>, ProtocolError> {
    if frame.0.encoded_len() > MAX_ENCODED_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge);
    }
    Ok(frame.0.encode_to_vec())
}

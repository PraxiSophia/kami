//! Private prost representation; kept in lockstep with proto/kami/m1/frame.proto.
//! Derives avoid a protoc/build-time dependency.
#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct WireFrame {
    #[prost(uint32, optional, tag = "1")]
    pub version: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub kind: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub message_id: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub conversation_id: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "5")]
    pub sender: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "6")]
    pub recipient: Option<Vec<u8>>,
    #[prost(uint64, optional, tag = "7")]
    pub sequence: Option<u64>,
    #[prost(uint64, optional, tag = "8")]
    pub created_at: Option<u64>,
    #[prost(bytes = "vec", optional, tag = "9")]
    pub payload: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "10")]
    pub context: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "11")]
    pub receipt_target: Option<Vec<u8>>,
    #[prost(uint32, optional, tag = "12")]
    pub mandatory_features: Option<u32>,
}

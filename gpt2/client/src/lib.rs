pub use gpt2_proto::gpt2::{GenerateRequest, GeneratedText};
use tonic::transport::Channel;

pub type Gpt2Client = gpt2_proto::gpt2::gpt2_client::Gpt2Client<Channel>;

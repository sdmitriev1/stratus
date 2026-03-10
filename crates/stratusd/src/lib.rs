pub mod config;
pub mod server;

pub mod proto {
    tonic::include_proto!("stratus.v1");
}

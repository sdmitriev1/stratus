pub mod config;
pub mod server;
pub mod vm_manager;

pub mod proto {
    tonic::include_proto!("stratus.v1");
}

use std::sync::Arc;
use tonic::transport::Server;
use crate::domain::compensation::*;
use crate::api::grpc::*;


use crate::api::grpc::proto::compensation::pay_run_service_server::PayRunServiceServer;


pub trait Repositories {
    
    type PayRunRepository: PayRunRepository;
    
}

pub fn grpc_router<R: Repositories + 'static>() -> Server {
    Server::builder()
        
        .add_service(PayRunServiceServer::new(
            PayRunGrpcService::<R>::new(),
        ))
        
}

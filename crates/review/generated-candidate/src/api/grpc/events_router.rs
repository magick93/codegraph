use std::sync::Arc;
use tonic::transport::Server;
use crate::domain::events::*;
use crate::api::grpc::*;


use crate::api::grpc::proto::events::public_event_service_server::PublicEventServiceServer;


pub trait Repositories {
    
    type PublicEventRepository: PublicEventRepository;
    
}

pub fn grpc_router<R: Repositories + 'static>() -> Server {
    Server::builder()
        
        .add_service(PublicEventServiceServer::new(
            PublicEventGrpcService::<R>::new(),
        ))
        
}

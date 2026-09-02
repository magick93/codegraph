use std::sync::Arc;
use tonic::transport::Server;
use crate::domain::rsvp::*;
use crate::api::grpc::*;


use crate::api::grpc::proto::rsvp::rsvp_service_server::RsvpServiceServer;


pub trait Repositories {
    
    type RsvpRepository: RsvpRepository;
    
}

pub fn grpc_router<R: Repositories + 'static>() -> Server {
    Server::builder()
        
        .add_service(RsvpServiceServer::new(
            RsvpGrpcService::<R>::new(),
        ))
        
}

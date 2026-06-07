use std::sync::Arc;
use tonic::transport::Server;
use crate::domain::recruiting::*;
use crate::api::grpc::*;


use crate::api::grpc::proto::recruiting::application_service_server::ApplicationServiceServer;

use crate::api::grpc::proto::recruiting::candidate_service_server::CandidateServiceServer;


pub trait Repositories {
    
    type ApplicationRepository: ApplicationRepository;
    
    type CandidateRepository: CandidateRepository;
    
}

pub fn grpc_router<R: Repositories + 'static>() -> Server {
    Server::builder()
        
        .add_service(ApplicationServiceServer::new(
            ApplicationGrpcService::<R>::new(),
        ))
        
        .add_service(CandidateServiceServer::new(
            CandidateGrpcService::<R>::new(),
        ))
        
}

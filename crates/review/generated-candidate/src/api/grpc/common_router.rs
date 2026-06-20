use std::sync::Arc;
use tonic::transport::Server;
use crate::domain::common::*;
use crate::api::grpc::*;


use crate::api::grpc::proto::common::code_service_server::CodeServiceServer;

use crate::api::grpc::proto::common::currency_code_list_service_server::CurrencyCodeListServiceServer;

use crate::api::grpc::proto::common::date_service_server::DateServiceServer;

use crate::api::grpc::proto::common::distribution_base_service_server::DistributionBaseServiceServer;

use crate::api::grpc::proto::common::effective_date_service_server::EffectiveDateServiceServer;

use crate::api::grpc::proto::common::formatted_date_time_service_server::FormattedDateTimeServiceServer;

use crate::api::grpc::proto::common::gender_code_list_service_server::GenderCodeListServiceServer;

use crate::api::grpc::proto::common::identifier_service_server::IdentifierServiceServer;

use crate::api::grpc::proto::common::name_service_server::NameServiceServer;

use crate::api::grpc::proto::common::person_base_service_server::PersonBaseServiceServer;

use crate::api::grpc::proto::common::position_schedule_type_code_list_service_server::PositionScheduleTypeCodeListServiceServer;

use crate::api::grpc::proto::common::string_type_array_service_server::StringTypeArrayServiceServer;

use crate::api::grpc::proto::common::amount_service_server::AmountServiceServer;

use crate::api::grpc::proto::common::process_history_item_service_server::ProcessHistoryItemServiceServer;

use crate::api::grpc::proto::common::process_history_service_server::ProcessHistoryServiceServer;


pub trait Repositories {
    
    type CodeRepository: CodeRepository;
    
    type CurrencyCodeListRepository: CurrencyCodeListRepository;
    
    type DateRepository: DateRepository;
    
    type DistributionBaseRepository: DistributionBaseRepository;
    
    type EffectiveDateRepository: EffectiveDateRepository;
    
    type FormattedDateTimeRepository: FormattedDateTimeRepository;
    
    type GenderCodeListRepository: GenderCodeListRepository;
    
    type IdentifierRepository: IdentifierRepository;
    
    type NameRepository: NameRepository;
    
    type PersonBaseRepository: PersonBaseRepository;
    
    type PositionScheduleTypeCodeListRepository: PositionScheduleTypeCodeListRepository;
    
    type StringTypeArrayRepository: StringTypeArrayRepository;
    
    type AmountRepository: AmountRepository;
    
    type ProcessHistoryItemRepository: ProcessHistoryItemRepository;
    
    type ProcessHistoryRepository: ProcessHistoryRepository;
    
}

pub fn grpc_router<R: Repositories + 'static>() -> Server {
    Server::builder()
        
        .add_service(CodeServiceServer::new(
            CodeGrpcService::<R>::new(),
        ))
        
        .add_service(CurrencyCodeListServiceServer::new(
            CurrencyCodeListGrpcService::<R>::new(),
        ))
        
        .add_service(DateServiceServer::new(
            DateGrpcService::<R>::new(),
        ))
        
        .add_service(DistributionBaseServiceServer::new(
            DistributionBaseGrpcService::<R>::new(),
        ))
        
        .add_service(EffectiveDateServiceServer::new(
            EffectiveDateGrpcService::<R>::new(),
        ))
        
        .add_service(FormattedDateTimeServiceServer::new(
            FormattedDateTimeGrpcService::<R>::new(),
        ))
        
        .add_service(GenderCodeListServiceServer::new(
            GenderCodeListGrpcService::<R>::new(),
        ))
        
        .add_service(IdentifierServiceServer::new(
            IdentifierGrpcService::<R>::new(),
        ))
        
        .add_service(NameServiceServer::new(
            NameGrpcService::<R>::new(),
        ))
        
        .add_service(PersonBaseServiceServer::new(
            PersonBaseGrpcService::<R>::new(),
        ))
        
        .add_service(PositionScheduleTypeCodeListServiceServer::new(
            PositionScheduleTypeCodeListGrpcService::<R>::new(),
        ))
        
        .add_service(StringTypeArrayServiceServer::new(
            StringTypeArrayGrpcService::<R>::new(),
        ))
        
        .add_service(AmountServiceServer::new(
            AmountGrpcService::<R>::new(),
        ))
        
        .add_service(ProcessHistoryItemServiceServer::new(
            ProcessHistoryItemGrpcService::<R>::new(),
        ))
        
        .add_service(ProcessHistoryServiceServer::new(
            ProcessHistoryGrpcService::<R>::new(),
        ))
        
}

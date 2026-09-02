use std::sync::Arc;
use tonic::{Request, Response, Status};
use crate::api::grpc::proto::common::event_base_service_server::EventBaseService;
use crate::domain::common::event_base::repository::EventBaseRepository;

pub struct EventBaseGrpcService<R: EventBaseRepository> {
    repo: Arc<R>,
}

impl<R: EventBaseRepository> EventBaseGrpcService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[tonic::async_trait]
impl<R: EventBaseRepository + 'static> EventBaseService for EventBaseGrpcService<R> {
    
    async fn create(
        &self,
        request: Request<EventBaseCreateRequest>,
    ) -> Result<Response<EventBase>, Status> {
        let req = request.into_inner();
        let cmd = CreateEventBaseCommand::from(req);
        let db = crate::app_state::get_db();
        let tx = db.begin().await.map_err(|e| Status::internal(e.to_string()))?;
        let id = self.repo
            .create(&tx, cmd)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        tx.commit().await.map_err(|e| Status::internal(e.to_string()))?;
        let response = self.repo.find_by_id(&db, id).await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("entity not found"))?
            .into();
        Ok(Response::new(response))
    }
    

    
    async fn get(
        &self,
        request: Request<GetEventBaseRequest>,
    ) -> Result<Response<EventBase>, Status> {
        let req = request.into_inner();
        let id = uuid::Uuid::parse_str(&req.id)
            .map_err(|_| Status::invalid_argument("invalid id"))?;
        let db = crate::app_state::get_db();
        let result = self.repo
            .find_by_id(&db, id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        match result {
            Some(entity) => Ok(Response::new(entity.into())),
            None => Err(Status::not_found("entity not found")),
        }
    }
    

    
    async fn update(
        &self,
        request: Request<UpdateEventBaseRequest>,
    ) -> Result<Response<EventBase>, Status> {
        let req = request.into_inner();
        let id = uuid::Uuid::parse_str(&req.id)
            .map_err(|_| Status::invalid_argument("invalid id"))?;
        let cmd = UpdateEventBaseCommand::from(req);
        let db = crate::app_state::get_db();
        let tx = db.begin().await.map_err(|e| Status::internal(e.to_string()))?;
        self.repo
            .update(&tx, id, cmd)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        tx.commit().await.map_err(|e| Status::internal(e.to_string()))?;
        let response = self.repo.find_by_id(&db, id).await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("entity not found"))?
            .into();
        Ok(Response::new(response))
    }
    

    
    async fn delete(
        &self,
        request: Request<DeleteEventBaseRequest>,
    ) -> Result<Response<prost_types::Empty>, Status> {
        let req = request.into_inner();
        let id = uuid::Uuid::parse_str(&req.id)
            .map_err(|_| Status::invalid_argument("invalid id"))?;
        let db = crate::app_state::get_db();
        let tx = db.begin().await.map_err(|e| Status::internal(e.to_string()))?;
        self.repo
            .delete(&tx, id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        tx.commit().await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(prost_types::Empty::default()))
    }
    

    
    async fn list(
        &self,
        request: Request<ListEventBaseRequest>,
    ) -> Result<Response<ListEventBaseResponse>, Status> {
        let req = request.into_inner();
        let db = crate::app_state::get_db();
        let filters: std::collections::HashMap<String, String> = req.filters.into_iter()
            .map(|f| (f.field, f.value))
            .collect();
        let (entities, total) = self.repo
            .list(&db, req.page_token.parse().unwrap_or(1), req.page_size as u64, &filters)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let data: Vec<EventBase> = entities.into_iter().map(Into::into).collect();
        Ok(Response::new(ListEventBaseResponse {
            data,
            total: total as i32,
            next_page_token: String::new(),
        }))
    }
    

    

    

    

    
}

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    routing::{get, post},
    Json, Router,
};
use k8s_openapi::api::batch::v1::Job;
use tracing::{debug, error, trace};

use crate::api::CreateJob;
use crate::State;

pub async fn start_server(state: Arc<State>) {
    debug!("Starting HTTP server");

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(handlers::root))
        .route(
            "/jobs",
            post({
                let shared_state = state.clone();
                move |req: Json<CreateJob>| handlers::create_job(req, shared_state)
            }),
        )
        .route("/jobs", get(move || handlers::jobs(Arc::clone(&state))));

    // run our app with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    debug!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("http serve failed");

    error!("server quit unexpectedly");
}

mod handlers {
    use super::*;

    pub(crate) async fn create_job(
        Json(request): Json<CreateJob>,
        state: Arc<State>,
    ) -> Json<Result<Job, ()>> {
        trace!(?request, "create job request");

        let result = crate::create_job(state.client.clone(), request)
            .await
            .map_err(|_| ());

        Json(result)
    }

    pub(crate) async fn jobs(state: Arc<State>) -> Json<Vec<Job>> {
        let jobs = crate::list_jobs(state.client.clone())
            .await
            .map_or_else(|_| vec![], |x| x.items);

        Json(jobs)
    }

    // basic handler that responds with a static string
    pub(crate) async fn root() -> &'static str {
        "Hello, World!"
    }
}

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    routing::{get, post},
    Json, Router,
};
use color_eyre::eyre;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, PostParams},
    runtime::utils::try_flatten_applied,
    Client, Error,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, instrument, trace};

mod job;

/// The default container image to use when none is provided in a `CreateJob` request.
const DEFAULT_IMAGE: &str = "ghcr.io/rwx-im/croc:edge";
/// The default namespace to use.
const DEFAULT_NAMESPACE: &str = "meta";
/// The controller name.
const CONTROLLER_NAME: &str = env!("CARGO_PKG_NAME");
/// The controller version.
const CONTROLLER_VERSION: &str = env!("CARGO_PKG_VERSION");

const UPLOAD_SCRIPT: &str = r#"
set -o errexit
set -o pipefail
set -o xtrace

filename=$(find /data -type f)

echo "Uploading file ${filename}"
name="$(basename "${filename}")"
hash=$(xxh128sum "$filename" | awk '{ print $1 }')
subpath="${hash:0:2}/${hash}/${name}"

aws --endpoint-url "${AWS_ENDPOINT}" s3 cp "${filename}" "s3://${S3_BUCKET}/${subpath}"

url="${PUBLIC_URL_PREFIX}${subpath}"
"#;

struct State {
    pub client: Client,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct CreateJob {
    pub image: Option<String>,
    pub args: Vec<String>,
    pub code: String,
}

async fn list_jobs(client: Client) -> Result<kube::core::ObjectList<Job>, Error> {
    let jobs: Api<Job> = Api::namespaced(client, DEFAULT_NAMESPACE);

    let lp = ListParams::default()
        .labels("app.kubernetes.io/component=croc,app.kubernetes.io/part-of=meta");

    let jobs = jobs.list(&lp).await?;

    Ok(jobs)
}

#[instrument(skip(client))]
async fn create_job(client: Client, request: CreateJob) -> eyre::Result<Job> {
    let image = request.image.unwrap_or_else(|| DEFAULT_IMAGE.to_owned());
    let my_job = serde_json::from_value(json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "generateName": "meta-croc-",
            "namespace": "meta",
            "labels": {
                "app.kubernetes.io/component": "croc",
                "app.kubernetes.io/created-by": CONTROLLER_NAME,
                "app.kubernetes.io/version": CONTROLLER_VERSION,
                "app.kubernetes.io/part-of": "meta",
            }
        },
        "spec": {
            "backoffLimit": 3i64,
            "template": {
                "metadata": {
                    "labels": {
                        "app.kubernetes.io/component": "croc",
                        "app.kubernetes.io/created-by": CONTROLLER_NAME,
                        "app.kubernetes.io/version": CONTROLLER_VERSION,
                        "app.kubernetes.io/part-of": "meta",
                    },
                    "annotations": {
                        "vault.security.banzaicloud.io/vault-role": "meta",
                        "vault.security.banzaicloud.io/vault-addr": "https://vault.default:8200",
                        "vault.security.banzaicloud.io/vault-tls-secret": "vault-tls",
                    }
                },
                "spec": {
                    "serviceAccountName": "default",
                    "volumes": [{
                        "name": "data",
                        "emptyDir": {}
                    }],
                    "initContainers": [{
                        "name": "croc",
                        "image": image,
                        "args": ["--relay", "croc.maero.dk", "--yes", request.code],
                        "volumeMounts": [{
                            "name": "data",
                            "mountPath": "/data"
                        }]
                    }],
                    "containers": [{
                        "name": "upload",
                        "image": "ghcr.io/mkroman/meta-utils:test",
                        "args": ["-c", UPLOAD_SCRIPT],
                        "env": [
                            {
                                "name": "AWS_ACCESS_KEY_ID",
                                "value": "vault:secret/data/meta/io#AWS_ACCESS_KEY_ID"
                            },
                            {
                                "name": "AWS_SECRET_ACCESS_KEY",
                                "value": "vault:secret/data/meta/io#AWS_SECRET_ACCESS_KEY"
                            },
                            {
                                "name": "AWS_ENDPOINT",
                                "value": "vault:secret/data/meta/io#AWS_ENDPOINT"
                            },
                            {
                                "name": "S3_BUCKET",
                                "value": "rwx"
                            },
                            {
                                "name": "PUBLIC_URL_PREFIX",
                                "value": "https://io.rwx.im/rwx/"
                            },
                        ],
                        "volumeMounts": [{
                            "name": "data",
                            "mountPath": "/data"
                        }]
                    }],
                    "restartPolicy": "Never",
                }
            },
            "ttlSecondsAfterFinished": 86400i64
        }
    }))
    .unwrap();

    debug!(?my_job, "creating");

    let jobs: Api<Job> = Api::namespaced(client, DEFAULT_NAMESPACE);
    let pp = PostParams::default();
    jobs.create(&pp, &my_job).await.map_err(|e| e.into())
}

/// Notifies meta that the job has completed.
pub async fn notify_job_status() {}

#[instrument(skip(client, tx))]
pub async fn watch_jobs(client: Client, tx: mpsc::UnboundedSender<Job>) -> eyre::Result<()> {
    trace!("Watching jobs");

    let api = Api::<Job>::namespaced(client, DEFAULT_NAMESPACE);
    let lp = ListParams::default()
        .labels("app.kubernetes.io/component=croc,app.kubernetes.io/part-of=meta,meta.rwx.im/notified!=true");
    let watcher = kube::runtime::watcher(api.clone(), lp);
    let mut apply_stream = try_flatten_applied(watcher).boxed();

    while let Some(job) = apply_stream.try_next().await? {
        let progress = job::progress(&job);

        debug!(?progress);

        if progress == job::Progress::Finished {
            if let Some(ref name) = job.metadata.name {
                debug!(
                    "Adding meta.rwx.im/notified label to {:?}",
                    job.metadata.name
                );

                api.patch(
                    name.as_str(),
                    &PatchParams::apply(CONTROLLER_NAME),
                    &Patch::Merge(json!({
                        "metadata": {
                            "labels": {
                                "meta.rwx.im/notified": "true"
                            }
                        }
                    })),
                )
                .await
                .unwrap();
            }

            tx.send(job)?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    // initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let client = Client::try_default().await?;

    debug!(
        "APIServer version: {:#?}",
        client.apiserver_version().await?
    );

    // Create an unbounded mpsc channel for receiving messages about jobs that have finished.
    let (tx, mut rx) = mpsc::unbounded_channel::<Job>();

    let state = Arc::new(State {
        client: client.clone(),
    });

    let watch_reporter = tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            debug!("job {:?} finished", job.metadata.name);
        }
    });

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(http::root))
        .route(
            "/jobs",
            post({
                let shared_state = state.clone();
                move |req: Json<CreateJob>| http::create_job(req, shared_state)
            }),
        )
        .route("/jobs", get(move || http::jobs(Arc::clone(&state))));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    debug!("listening on {}", addr);

    let server = tokio::spawn(async move {
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
    });

    let _ = tokio::join!(server, watch_jobs(client.clone(), tx), watch_reporter);

    Ok(())
}

mod http {
    use super::*;

    pub(crate) async fn create_job(
        Json(request): Json<CreateJob>,
        state: Arc<State>,
    ) -> Json<Result<Job, ()>> {
        trace!(?request, "create job request");

        let result = super::create_job(state.client.clone(), request)
            .await
            .map_err(|_| ());

        Json(result)
    }

    pub(crate) async fn jobs(state: Arc<State>) -> Json<Vec<Job>> {
        let jobs = list_jobs(state.client.clone())
            .await
            .map_or_else(|_| vec![], |x| x.items);

        Json(jobs)
    }

    // basic handler that responds with a static string
    pub(crate) async fn root() -> &'static str {
        "Hello, World!"
    }
}

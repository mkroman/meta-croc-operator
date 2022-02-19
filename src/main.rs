use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, PostParams},
    runtime::utils::try_flatten_applied,
    Client,
};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, instrument, trace};

mod api;
mod cli;
mod error;
mod http;
mod job;

use api::CreateJob;
pub use error::Error;

pub struct State {
    pub client: Client,
}

async fn list_jobs(client: Client) -> Result<kube::core::ObjectList<Job>, Error> {
    let jobs: Api<Job> = Api::namespaced(client, "meta");

    let lp = ListParams::default()
        .labels("app.kubernetes.io/component=croc,app.kubernetes.io/part-of=meta");

    let jobs = jobs.list(&lp).await?;

    Ok(jobs)
}

#[instrument(skip(client))]
async fn create_job(client: Client, request: CreateJob) -> Result<Job, Error> {
    let my_job = job::create(request)?;

    debug!(?my_job, "creating");

    let jobs: Api<Job> = Api::namespaced(client, "meta");
    let pp = PostParams::default();
    jobs.create(&pp, &my_job).await.map_err(|e| e.into())
}

/// Notifies meta that the job has completed.
pub async fn notify_job_status() {}

#[instrument(skip(client, tx))]
pub async fn watch_jobs(client: Client, tx: mpsc::UnboundedSender<Job>) -> Result<(), Error> {
    trace!("Watching jobs");

    let api = Api::<Job>::namespaced(client, "meta");
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
                    &PatchParams::apply("meta"),
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

            tx.send(job).map_err(|_| Error::MpscSendFailed)?;
        }
    }

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    init_tracing();

    let _opts = cli::Opts::parse();

    // Initialize a Kubernetes client and infer the config from the environment.
    let client = Client::try_default().await?;
    let apiserver_version = client.apiserver_version().await?;

    debug!(?apiserver_version, "connected");

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

    let http_server = http::start_server(state.clone());

    let _ = tokio::join!(http_server, watch_jobs(client.clone(), tx), watch_reporter);

    Ok(())
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes API error")]
    KubeApi(#[from] kube::Error),
    #[error("Kubernetes watcher error")]
    KubeWatcher(#[from] kube::runtime::watcher::Error),
    #[error("Could not send mpsc message")]
    MpscSendFailed,
}

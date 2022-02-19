use k8s_openapi::api::batch::v1::Job;
use serde_json::json;
use tracing::debug;

use crate::api::CreateJob;
use crate::Error;

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Progress {
    Finished,
    Failed,
    Running,
    Pending,
}

/// Returns true if the given jobs containers have all run successfully.
pub fn progress(job: &Job) -> Progress {
    let status = job.status.as_ref().expect("job is missing status");

    debug!("{:?}", status);

    if status.start_time.is_none() {
        return Progress::Pending;
    } else if status.active == None {
        if status.failed == None {
            return Progress::Finished;
        } else {
            return Progress::Failed;
        }
    }

    Progress::Running
}

/// Returns a JSON manifest from a given `request`.
pub fn create(request: CreateJob) -> Result<Job, Error> {
    let image = request.image.unwrap_or_else(|| DEFAULT_IMAGE.to_owned());

    serde_json::from_value(json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "generateName": "meta-croc-",
            "namespace": DEFAULT_NAMESPACE,
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
    .map_err(Error::JobManifestFailed)
}

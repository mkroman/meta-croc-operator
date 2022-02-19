use k8s_openapi::api::batch::v1::Job;
use tracing::debug;

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

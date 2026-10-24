//! One-shot, path-scoped fault injection for cancellation regression tests.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, LazyLock, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;

use crate::error::{AppError, Result};

struct Gate {
    reached: oneshot::Sender<()>,
    resume: mpsc::Receiver<bool>,
}

static GATES: LazyLock<Mutex<HashMap<PathBuf, Gate>>> = LazyLock::new(Mutex::default);

pub(crate) struct WriteGate {
    path: PathBuf,
    reached: oneshot::Receiver<()>,
    resume: Option<mpsc::Sender<bool>>,
}

impl WriteGate {
    pub(crate) fn arm(path: &Path) -> Self {
        let (reached_tx, reached) = oneshot::channel();
        let (resume, resume_rx) = mpsc::channel();
        assert!(GATES
            .lock()
            .unwrap()
            .insert(
                path.to_owned(),
                Gate {
                    reached: reached_tx,
                    resume: resume_rx,
                }
            )
            .is_none());
        Self {
            path: path.to_owned(),
            reached,
            resume: Some(resume),
        }
    }

    pub(crate) async fn wait(&mut self) {
        tokio::time::timeout(Duration::from_secs(10), &mut self.reached)
            .await
            .expect("write did not reach rename")
            .expect("writer dropped gate");
    }

    pub(crate) fn fail(mut self) {
        let _ = self.resume.take().unwrap().send(true);
    }
}

impl Drop for WriteGate {
    fn drop(&mut self) {
        GATES.lock().unwrap().remove(&self.path);
        if let Some(resume) = self.resume.take() {
            let _ = resume.send(false);
        }
    }
}

pub(super) fn before_rename(path: &Path) -> Result<()> {
    let gate = GATES.lock().unwrap().remove(path);
    if let Some(gate) = gate {
        let _ = gate.reached.send(());
        if gate
            .resume
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| AppError::Database(format!("Write gate timed out: {error}")))?
        {
            return Err(AppError::Database("Injected write failure".into()));
        }
    }
    Ok(())
}

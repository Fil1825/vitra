use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde_json::json;
use tokio::process::{Child, Command};
use tokio::time::timeout;

use crate::client::{Client, Events};
use crate::error::Error;

pub struct SpawnOptions {
    pub binary: PathBuf,
    /// Command-line arguments; the IPC connection arguments are added automatically.
    pub args: Vec<OsString>,
    /// Windows only: named pipes are called `\\.\pipe\<prefix>-<pid>`.
    pub pipe_prefix: String,
    /// How long to wait for mpv to answer its first request.
    pub startup_timeout: Duration,
}

/// A running mpv child process. Its IPC connection is returned separately by [`spawn`] so it
/// can be shared while the process is being waited on.
///
/// [`spawn`]: MpvProcess::spawn
pub struct MpvProcess {
    child: Child,
    version: String,
}

impl MpvProcess {
    /// Starts mpv and connects to it. The child is killed if this handle is dropped.
    ///
    /// Fails with [`Error::Exited`] if mpv quits during startup, e.g. because it doesn't know
    /// one of the arguments.
    pub async fn spawn(options: SpawnOptions) -> Result<(MpvProcess, Client, Events), Error> {
        let mut command = Command::new(&options.binary);
        command
            .args(&options.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // mpv.exe is a console program: don't flash a console window.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let (mut child, client, events) = connect(command, &options).await?;

        // A first request proves mpv is up and talking.
        match timeout(
            options.startup_timeout,
            client.get_property::<String>("mpv-version"),
        )
        .await
        {
            Ok(Ok(version)) => Ok((MpvProcess { child, version }, client, events)),
            Ok(Err(err)) => Err(exit_status(&mut child).await.map_or(err, Error::Exited)),
            Err(_) => {
                let _ = child.kill().await;
                Err(Error::Timeout)
            }
        }
    }

    /// The `mpv-version` string, e.g. `mpv v0.41.0`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Asks mpv to quit, then kills it if it hasn't exited within `grace`.
    pub async fn shutdown(mut self, client: &Client, grace: Duration) {
        let _ = timeout(grace, client.command(vec![json!("quit")])).await;
        if timeout(grace, self.child.wait()).await.is_err() {
            let _ = self.child.kill().await;
        }
    }

    /// Resolves when the mpv process exits (crash or quit).
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }
}

/// The exit status if mpv has exited (or does so momentarily).
async fn exit_status(child: &mut Child) -> Option<ExitStatus> {
    timeout(Duration::from_secs(1), child.wait())
        .await
        .ok()?
        .ok()
}

/// Unix: hand mpv one end of a socket pair as its stdin (`--input-ipc-client=fd://0`).
/// mpv quits as soon as that connection closes, so it can never outlive the app, even when
/// the app crashes or is killed. No socket file is created either.
#[cfg(unix)]
async fn connect(
    mut command: Command,
    _options: &SpawnOptions,
) -> Result<(Child, Client, Events), Error> {
    use std::os::fd::OwnedFd;

    let (ours, theirs) = std::os::unix::net::UnixStream::pair()?;
    ours.set_nonblocking(true)?;
    command
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .arg("--input-ipc-client=fd://0");
    let child = command.spawn()?;
    // `command` still holds mpv's end of the pair; dropping it leaves mpv the only owner.
    drop(command);

    let (client, events) = Client::new(tokio::net::UnixStream::from_std(ours)?);
    Ok((child, client, events))
}

/// Windows: mpv serves a named pipe; connect once it exists.
#[cfg(windows)]
async fn connect(
    mut command: Command,
    options: &SpawnOptions,
) -> Result<(Child, Client, Events), Error> {
    use tokio::net::windows::named_pipe::ClientOptions;
    use tokio::time::{Instant, sleep};

    let pipe = format!(r"\\.\pipe\{}-{}", options.pipe_prefix, std::process::id());
    command
        .stdin(Stdio::null())
        .arg(format!("--input-ipc-server={pipe}"));
    let mut child = command.spawn()?;

    let deadline = Instant::now() + options.startup_timeout;
    let stream = loop {
        match ClientOptions::new().open(&pipe) {
            Ok(stream) => break stream,
            Err(err) => {
                if let Some(status) = child.try_wait()? {
                    return Err(Error::Exited(status));
                }
                if Instant::now() >= deadline {
                    let _ = child.kill().await;
                    log::warn!("mpv IPC pipe never came up: {err}");
                    return Err(Error::Timeout);
                }
                sleep(Duration::from_millis(25)).await;
            }
        }
    };
    let (client, events) = Client::new(stream);
    Ok((child, client, events))
}

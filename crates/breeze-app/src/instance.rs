//! One application process per data directory, with OS opens handed to its window.
//! All filesystem and socket work happens before GPUI starts or on the listener thread.

use anyhow::{Context, Result, ensure};
use futures::channel::mpsc::{self, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_PATHS: usize = 256;
const MAX_QUEUED_REQUESTS: usize = 8;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize, Serialize)]
struct Endpoint {
    port: u16,
    token: String,
}

#[derive(Deserialize, Serialize)]
struct Request {
    token: String,
    paths: Vec<PathBuf>,
}

pub(super) enum Launch {
    Primary(Instance, Receiver<Vec<PathBuf>>),
    Forwarded,
}

pub(super) struct Instance {
    // The process keeps ownership even if its listener fails. Never delete the
    // lock file: a waiting process may already have opened that inode.
    _lock: File,
    address: SocketAddr,
    stopping: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        // Wake a blocking accept so shutdown finishes before releasing the lock.
        let _ = TcpStream::connect_timeout(&self.address, IO_TIMEOUT);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

/// Claim leadership before creating a PDF worker, state worker, or native window.
/// A follower exits only after the existing process accepts its request.
pub(super) fn start(root: &Path, paths: Vec<PathBuf>) -> Result<Launch> {
    ensure!(
        paths.len() <= MAX_PATHS,
        "Too many documents requested at once"
    );
    fs::create_dir_all(root).context("Cannot create the Breeze data directory")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("instance.lock"))
        .context("Cannot open the Breeze process lock")?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            forward(root, paths).context("Cannot hand the open request to the running Breeze")?;
            return Ok(Launch::Forwarded);
        }
        Err(error) => return Err(error.into()),
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    // A random, user-local endpoint token prevents other loopback clients from
    // submitting paths without access to this application's data directory.
    let nonce = tempfile::Builder::new().rand_bytes(32).tempfile_in(root)?;
    let token = nonce
        .path()
        .file_name()
        .context("Missing endpoint token")?
        .to_str()
        .context("Invalid endpoint token")?
        .to_owned();
    let endpoint = Endpoint {
        port: address.port(),
        token,
    };
    let endpoint_path = root.join("instance.json");
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut file, &endpoint)?;
    file.as_file().sync_all()?;
    file.persist(&endpoint_path).map_err(|error| error.error)?;
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = stopping.clone();
    let (mut sender, receiver) = mpsc::channel(MAX_QUEUED_REQUESTS);
    let handle = thread::Builder::new()
        .name("breeze-open-requests".into())
        .spawn(move || {
            for connection in listener.incoming() {
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                match connection {
                    Ok(mut stream) => {
                        if let Err(error) = receive(&mut stream, &endpoint.token, &mut sender) {
                            eprintln!("Breeze rejected an open request: {error:#}");
                        }
                    }
                    Err(error) => {
                        eprintln!("Breeze open-request listener failed: {error}");
                        break;
                    }
                }
            }
            let _ = fs::remove_file(endpoint_path);
        })?;
    Ok(Launch::Primary(
        Instance {
            _lock: lock,
            address,
            stopping,
            listener: Some(handle),
        },
        receiver,
    ))
}

fn configure(stream: &TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    Ok(())
}

fn receive(stream: &mut TcpStream, token: &str, sender: &mut Sender<Vec<PathBuf>>) -> Result<()> {
    configure(stream)?;
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    ensure!(size <= MAX_REQUEST_BYTES, "Open request exceeds size limit");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(request.token == token, "Open request token does not match");
    ensure!(
        request.paths.len() <= MAX_PATHS,
        "Too many documents requested at once"
    );
    ensure!(
        request.paths.iter().all(|path| path.is_absolute()),
        "Open paths must be absolute"
    );
    sender
        .try_send(request.paths)
        .context("The open-request queue is unavailable or full")?;
    // Acknowledgement means queued, including while the first window restores.
    stream.write_all(&[1])?;
    Ok(())
}

fn forward(root: &Path, paths: Vec<PathBuf>) -> Result<()> {
    // Resolve relative arguments in the launching process's working directory.
    // Existence and document validation remain the reader's background work.
    let paths = paths
        .into_iter()
        .map(std::path::absolute)
        .collect::<std::io::Result<Vec<_>>>()?;
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let (endpoint, mut stream) = loop {
        let attempt = (|| -> Result<_> {
            let file = File::open(root.join("instance.json"))?;
            let endpoint: Endpoint = serde_json::from_reader(file.take(4096))?;
            let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
            let stream = TcpStream::connect_timeout(&address, IO_TIMEOUT)?;
            Ok((endpoint, stream))
        })();
        match attempt {
            Ok(connected) => break connected,
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    };
    configure(&stream)?;
    let bytes = serde_json::to_vec(&Request {
        token: endpoint.token,
        paths,
    })?;
    ensure!(
        bytes.len() <= MAX_REQUEST_BYTES,
        "Open request exceeds size limit"
    );
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    let mut acknowledgement = [0];
    stream.read_exact(&mut acknowledgement)?;
    ensure!(
        acknowledgement == [1],
        "Running Breeze did not accept the open request"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_tests;

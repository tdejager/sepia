//! Records the browser's own screencast into numbered PNG frames.
//!
//! Sepia attaches a second DevTools client to the page agent-browser is
//! driving and asks Chrome for `Page.startScreencast`. Chrome pushes a lossless
//! PNG whenever the page repaints; a recorder thread samples the newest one on
//! a fixed `1 / fps` clock, so the frames on disk are real-time footage at the
//! output frame rate.
//!
//! Chrome only sends the next frame once the previous one is acknowledged. The
//! recorder acknowledges once per clock tick, which caps Chrome's PNG encoding
//! at the output frame rate instead of the compositor's.

use std::{
    fs, io,
    net::TcpStream,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use miette::{Result, bail, miette};
use serde::Deserialize;
use serde_json::json;
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

use crate::{ResultContextExt, browser::Recording, session::SessionPaths};

/// How long Chrome gets to deliver the first frame, and the recorder to write
/// any later one, before the run fails instead of hanging.
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

/// `Page.startScreencast` is always the first command on the connection.
const START_SCREENCAST_ID: u64 = 1;

/// A screencast of one page, written as `frame-NNNNNN.png` files.
///
/// Frames are only written while [`Recording::roll`] is waiting for them;
/// between rolls the recorder keeps the newest frame fresh but writes nothing.
pub struct ScreencastRecording {
    /// Total frames requested so far; the recorder writes up to this many.
    requested: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    written: mpsc::Receiver<()>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl ScreencastRecording {
    /// Attach to the page `target_id` of the browser behind `cdp_url` (a
    /// `ws://host:port/...` DevTools endpoint) and start its screencast.
    /// Returns once Chrome has delivered the first frame.
    pub fn start(cdp_url: &str, target_id: &str, paths: &SessionPaths, fps: u32) -> Result<Self> {
        let authority = devtools_authority(cdp_url)?;
        let stream = TcpStream::connect(authority)
            .with_context(|| format!("failed to connect to the browser at {authority}"))?;
        stream
            .set_read_timeout(Some(FRAME_TIMEOUT))
            .context("failed to configure the DevTools connection")?;
        // Frames are whole PNGs in one message; the default 16 MiB frame cap is
        // too small for large viewports.
        let config = WebSocketConfig::default()
            .max_message_size(None)
            .max_frame_size(None);
        let (socket, _) = tungstenite::client::client_with_config(
            format!("ws://{authority}/devtools/page/{target_id}"),
            stream,
            Some(config),
        )
        .map_err(|err| miette!("failed to open a DevTools session for the page: {err}"))?;

        let requested = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (written_tx, written) = mpsc::channel();
        let mut recorder = Recorder {
            socket,
            next_id: START_SCREENCAST_ID,
            paths: paths.clone(),
            interval: Duration::from_secs_f64(1.0 / f64::from(fps.max(1))),
            requested: Arc::clone(&requested),
            stop: Arc::clone(&stop),
            written: written_tx,
            latest: String::new(),
            fresh: false,
            pending_ack: None,
            png: Vec::new(),
            last_frame: None,
        };
        recorder.start()?;

        Ok(Self {
            requested,
            stop,
            written,
            thread: Some(thread::spawn(move || recorder.run())),
        })
    }

    /// Why the recorder thread ended while frames were still expected.
    fn recorder_error(&mut self) -> miette::Report {
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(Err(err))) => err,
            _ => miette!("the screencast recorder stopped unexpectedly"),
        }
    }
}

impl Recording for ScreencastRecording {
    fn roll(&mut self, frames: u32, on_frame: &mut dyn FnMut()) -> Result<()> {
        self.requested.fetch_add(frames, Ordering::Relaxed);
        for _ in 0..frames {
            match self.written.recv_timeout(FRAME_TIMEOUT) {
                Ok(()) => on_frame(),
                Err(RecvTimeoutError::Timeout) => bail!(
                    "the screencast stalled: no frame was written for {}s",
                    FRAME_TIMEOUT.as_secs()
                ),
                Err(RecvTimeoutError::Disconnected) => return Err(self.recorder_error()),
            }
        }
        Ok(())
    }
}

impl Drop for ScreencastRecording {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The `host:port` of a `ws://host:port/...` DevTools URL.
fn devtools_authority(cdp_url: &str) -> Result<&str> {
    let authority = cdp_url
        .strip_prefix("ws://")
        .and_then(|rest| rest.split('/').next())
        .filter(|authority| !authority.is_empty());
    authority.with_context(|| {
        format!(
            "Sepia records over a local `ws://` DevTools endpoint, but the browser reported `{cdp_url}`"
        )
    })
}

#[derive(Deserialize)]
struct CdpMessage<'a> {
    id: Option<u64>,
    #[serde(borrow)]
    method: Option<std::borrow::Cow<'a, str>>,
    error: Option<CdpError>,
    #[serde(borrow)]
    params: Option<FrameParams<'a>>,
}

#[derive(Deserialize)]
struct CdpError {
    message: String,
}

/// The fields of a `Page.screencastFrame` event Sepia uses. Optional so other
/// events' params still parse.
#[derive(Deserialize)]
struct FrameParams<'a> {
    #[serde(borrow)]
    data: Option<std::borrow::Cow<'a, str>>,
    #[serde(rename = "sessionId")]
    session_id: Option<i64>,
}

struct Recorder {
    socket: WebSocket<TcpStream>,
    next_id: u64,
    paths: SessionPaths,
    interval: Duration,
    requested: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    written: mpsc::Sender<()>,
    /// Base64 PNG of the newest frame Chrome sent.
    latest: String,
    /// Whether `latest` is newer than the decoded `png`.
    fresh: bool,
    /// Screencast session of a received frame Chrome is still waiting on.
    pending_ack: Option<i64>,
    /// Decoded bytes of the last frame written.
    png: Vec<u8>,
    last_frame: Option<PathBuf>,
}

impl Recorder {
    /// Ask Chrome for frames and wait for the first, so every later tick has
    /// something to write.
    fn start(&mut self) -> Result<()> {
        self.send(
            "Page.startScreencast",
            json!({ "format": "png", "everyNthFrame": 1 }),
        )?;
        let deadline = Instant::now() + FRAME_TIMEOUT;
        while !self.fresh {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                bail!(
                    "the browser sent no screencast frame within {}s",
                    FRAME_TIMEOUT.as_secs()
                );
            };
            self.read_for(remaining)?;
        }
        Ok(())
    }

    fn run(mut self) -> Result<()> {
        let mut next_tick = Instant::now() + self.interval;
        let mut frames = 0;
        while !self.stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if let Some(remaining) = next_tick.checked_duration_since(now)
                && !remaining.is_zero()
            {
                self.read_for(remaining)?;
                continue;
            }
            next_tick += self.interval;
            if next_tick < now {
                // Stalled past a whole tick: resume the cadence, do not burst.
                next_tick = now + self.interval;
            }
            if frames < self.requested.load(Ordering::Relaxed) {
                frames += 1;
                self.write_frame(frames)?;
                if self.written.send(()).is_err() {
                    break;
                }
            }
            if let Some(session) = self.pending_ack.take() {
                self.send("Page.screencastFrameAck", json!({ "sessionId": session }))?;
            }
        }
        // Best effort: the browser drops the screencast with the connection anyway.
        let _ = self.send("Page.stopScreencast", json!({}));
        let _ = self.socket.close(None);
        Ok(())
    }

    /// Write the newest frame as video frame `number`. A page that has not
    /// repainted since the last tick reuses the previous file.
    fn write_frame(&mut self, number: u32) -> Result<()> {
        let path = self.paths.frame_path(number);
        if self.fresh {
            self.png.clear();
            STANDARD
                .decode_vec(&self.latest, &mut self.png)
                .context("the browser sent an undecodable screencast frame")?;
            self.fresh = false;
        } else if let Some(previous) = &self.last_frame
            && fs::hard_link(previous, &path).is_ok()
        {
            self.last_frame = Some(path);
            return Ok(());
        }
        fs::write(&path, &self.png)
            .with_context(|| format!("failed to write frame {}", path.display()))?;
        self.last_frame = Some(path);
        Ok(())
    }

    /// Handle at most one incoming message, waiting up to `timeout` for it.
    fn read_for(&mut self, timeout: Duration) -> Result<()> {
        self.socket
            .get_ref()
            // A zero timeout is rejected, so wait at least a millisecond.
            .set_read_timeout(Some(timeout.max(Duration::from_millis(1))))
            .context("failed to configure the DevTools connection")?;
        match self.socket.read() {
            Ok(Message::Text(text)) => self.handle(text.as_str()),
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                bail!("the browser closed the screencast connection")
            }
            Ok(_) => Ok(()),
            Err(tungstenite::Error::Io(err))
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(())
            }
            Err(err) => Err(err).context("the screencast connection failed"),
        }
    }

    fn handle(&mut self, text: &str) -> Result<()> {
        let message: CdpMessage = serde_json::from_str(text)
            .context("the browser sent an unreadable DevTools message")?;
        if message.id == Some(START_SCREENCAST_ID)
            && let Some(error) = message.error
        {
            bail!(
                "the browser refused to start a screencast: {}",
                error.message
            );
        }
        if message.method.as_deref() == Some("Page.screencastFrame")
            && let Some(FrameParams {
                data: Some(data),
                session_id: Some(session),
            }) = message.params
        {
            self.latest.clear();
            self.latest.push_str(&data);
            self.fresh = true;
            self.pending_ack = Some(session);
        }
        Ok(())
    }

    fn send(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        let command = json!({ "id": id, "method": method, "params": params });
        self.socket
            .send(Message::text(command.to_string()))
            .with_context(|| format!("failed to send `{method}` to the browser"))
    }
}

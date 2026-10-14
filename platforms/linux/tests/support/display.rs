//! A private headless GTK display: `gtk4-broadwayd` plus a minimal client.
//!
//! Without a browser attached, GDK's Broadway backend throttles every window
//! to one frame per second (it waits a second after each frame, see
//! `_gdk_broadway_roundtrip_notify`), so animations and tick callbacks crawl.
//! A connected client lifts that: the small client here speaks just enough of
//! the protocol to acknowledge each frame's roundtrip, answer grabs and
//! configures, and report a screen large enough for phone-sized windows.
//!
//! Shared by the integration tests (`tests/support`), the example tools and
//! the crate's own GTK unit tests (`src/gtk_test.rs`); it depends on the
//! standard library only.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The screen the client reports; windows larger than it would be clamped.
const SCREEN: (u32, u32) = (1600, 1600);

/// A running display; stopped when dropped (or, if never dropped, when the process exits).
pub struct Display {
    daemon: Child,
    socket: PathBuf,
}

impl Drop for Display {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        // Killed, the daemon leaves its socket behind.
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Points GDK at a fresh Broadway display unless `GDK_BACKEND=broadway`
/// already names one (then `Some(None)`). `None` when no display can start.
/// Call before `gtk::init`.
pub fn start() -> Option<Option<Display>> {
    if std::env::var("GDK_BACKEND").as_deref() == Ok("broadway") {
        return Some(None);
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    // Displays 100–999 stay clear of the low ones people pick by hand.
    let first = 100 + std::process::id() % 800;
    for attempt in 0..8 {
        let display = first + attempt * 97 % 800;
        if let Some(d) = launch(display, &runtime_dir) {
            std::env::set_var("GDK_BACKEND", "broadway");
            std::env::set_var("BROADWAY_DISPLAY", format!(":{display}"));
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("DISPLAY");
            return Some(Some(d));
        }
    }
    None
}

fn launch(display: u32, runtime_dir: &std::path::Path) -> Option<Display> {
    let socket = runtime_dir.join(format!("broadway{}.socket", display + 1));
    // A socket left by a killed daemon would pass for this one before it listens.
    let _ = std::fs::remove_file(&socket);
    let port = 20000 + display;
    let daemon = Command::new("gtk4-broadwayd")
        .arg(format!(":{display}"))
        .args(["--port", &port.to_string(), "--address", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut d = Display { daemon, socket };
    let deadline = Instant::now() + Duration::from_secs(10);
    // Listening, not merely created: the file may appear before `listen`.
    while UnixStream::connect(&d.socket).is_err() {
        // Exited: the display or the port is taken.
        if d.daemon.try_wait().ok().flatten().is_some() || Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    watchdog(&d);
    if let Err(e) = Client::connect(port) {
        eprintln!("broadway client: {e}; frames stay throttled to 1 fps");
    }
    Some(d)
}

/// Stops the daemon once this process is gone, even if it never drops the
/// [`Display`] (statics, aborts).
fn watchdog(d: &Display) {
    let script = format!(
        "while kill -0 {pid} 2>/dev/null; do sleep 1; done; kill {daemon} 2>/dev/null; rm -f '{socket}'",
        pid = std::process::id(),
        daemon = d.daemon.id(),
        socket = d.socket.display()
    );
    let _ = Command::new("sh").arg("-c").arg(script).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

// Server → client operations (gdk/broadway/broadway-protocol.h).
const OP_GRAB_POINTER: u8 = 0;
const OP_UNGRAB_POINTER: u8 = 1;
const OP_NEW_SURFACE: u8 = 2;
const OP_SHOW_SURFACE: u8 = 3;
const OP_HIDE_SURFACE: u8 = 4;
const OP_RAISE_SURFACE: u8 = 5;
const OP_LOWER_SURFACE: u8 = 6;
const OP_DESTROY_SURFACE: u8 = 7;
const OP_MOVE_RESIZE: u8 = 8;
const OP_SET_TRANSIENT_FOR: u8 = 9;
const OP_DISCONNECTED: u8 = 10;
const OP_SET_SHOW_KEYBOARD: u8 = 12;
const OP_UPLOAD_TEXTURE: u8 = 13;
const OP_RELEASE_TEXTURE: u8 = 14;
const OP_SET_NODES: u8 = 15;
const OP_ROUNDTRIP: u8 = 16;

// Client → server events.
const EVENT_GRAB_NOTIFY: u32 = 9;
const EVENT_UNGRAB_NOTIFY: u32 = 10;
const EVENT_CONFIGURE_NOTIFY: u32 = 11;
const EVENT_SCREEN_SIZE_CHANGED: u32 = 12;
const EVENT_ROUNDTRIP_NOTIFY: u32 = 14;

/// What a browser would answer, without drawing anything.
struct Client {
    stream: TcpStream,
    serial: u32,
    /// Surface id → `[x, y, width, height]`.
    surfaces: HashMap<u32, [i32; 4]>,
}

impl Client {
    fn connect(port: u32) -> std::io::Result<()> {
        let mut stream = TcpStream::connect(("127.0.0.1", port as u16))?;
        write!(
            stream,
            "GET /socket HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: bmF0aXZlLXNoZWxsLXRlc3Q=\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: broadway\r\n\r\n"
        )?;
        // Skip the handshake answer.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte)?;
            head.push(byte[0]);
        }
        let mut client = Client { stream, serial: 0, surfaces: HashMap::new() };
        client.send(EVENT_SCREEN_SIZE_CHANGED, &[SCREEN.0, SCREEN.1, 1])?;
        std::thread::Builder::new().name("broadway-client".into()).spawn(move || {
            while let Ok(message) = client.read_message() {
                if client.handle(&message).is_err() {
                    break;
                }
            }
        })?;
        Ok(())
    }

    /// One WebSocket message (fragments joined); pings and text are skipped.
    fn read_message(&mut self) -> std::io::Result<Vec<u8>> {
        let mut message = Vec::new();
        loop {
            let mut head = [0u8; 2];
            self.stream.read_exact(&mut head)?;
            let (fin, opcode) = (head[0] & 0x80 != 0, head[0] & 0x0f);
            let mut len = (head[1] & 0x7f) as u64;
            if len == 126 {
                let mut b = [0u8; 2];
                self.stream.read_exact(&mut b)?;
                len = u16::from_be_bytes(b) as u64;
            } else if len == 127 {
                let mut b = [0u8; 8];
                self.stream.read_exact(&mut b)?;
                len = u64::from_be_bytes(b);
            }
            let mut mask = [0u8; 4];
            if head[1] & 0x80 != 0 {
                self.stream.read_exact(&mut mask)?;
            }
            let mut payload = vec![0u8; len as usize];
            self.stream.read_exact(&mut payload)?;
            payload.iter_mut().enumerate().for_each(|(i, b)| *b ^= mask[i % 4]);
            match opcode {
                0x8 => return Err(std::io::ErrorKind::ConnectionAborted.into()),
                0x0 | 0x2 => message.extend_from_slice(&payload),
                _ => continue,
            }
            if fin {
                return Ok(message);
            }
        }
    }

    fn send(&mut self, event: u32, args: &[u32]) -> std::io::Result<()> {
        // `[type, serial, time, args…]` as big-endian 32-bit words; time 0 means "now".
        let mut payload = Vec::with_capacity(12 + args.len() * 4);
        for word in [event, self.serial, 0].iter().chain(args) {
            payload.extend_from_slice(&word.to_be_bytes());
        }
        // A client frame is masked; the zero mask leaves the payload as is.
        let mut frame = vec![0x82, 0x80 | payload.len() as u8, 0, 0, 0, 0];
        frame.extend_from_slice(&payload);
        self.stream.write_all(&frame)
    }

    fn handle(&mut self, message: &[u8]) -> std::io::Result<()> {
        let mut r = Reader { data: message, pos: 0 };
        while r.pos < message.len() {
            let op = r.u8();
            self.serial = r.u32();
            match op {
                OP_DISCONNECTED => {}
                OP_UNGRAB_POINTER => self.send(EVENT_UNGRAB_NOTIFY, &[])?,
                OP_GRAB_POINTER => {
                    r.skip(3);
                    self.send(EVENT_GRAB_NOTIFY, &[])?;
                }
                OP_NEW_SURFACE => {
                    let id = r.u16() as u32;
                    let geometry = [r.i16(), r.i16(), r.u16() as i32, r.u16() as i32];
                    self.surfaces.insert(id, geometry);
                }
                OP_SHOW_SURFACE | OP_HIDE_SURFACE | OP_RAISE_SURFACE | OP_LOWER_SURFACE | OP_SET_SHOW_KEYBOARD => r.skip(2),
                OP_DESTROY_SURFACE => {
                    self.surfaces.remove(&(r.u16() as u32));
                }
                OP_SET_TRANSIENT_FOR => r.skip(4),
                OP_MOVE_RESIZE => {
                    let id = r.u16() as u32;
                    let flags = r.u8();
                    let g = self.surfaces.entry(id).or_insert([0; 4]);
                    if flags & 1 != 0 {
                        g[0] = r.i16();
                        g[1] = r.i16();
                    }
                    if flags & 2 != 0 {
                        g[2] = r.u16() as i32;
                        g[3] = r.u16() as i32;
                    }
                    let g = *g;
                    self.send(EVENT_CONFIGURE_NOTIFY, &[id, g[0] as u32, g[1] as u32, g[2] as u32, g[3] as u32])?;
                }
                OP_UPLOAD_TEXTURE => {
                    r.skip(4);
                    let size = r.u32() as usize;
                    r.skip(size);
                }
                OP_RELEASE_TEXTURE => r.skip(4),
                OP_SET_NODES => {
                    r.skip(2);
                    let words = r.u32() as usize;
                    r.skip(words * 4);
                }
                OP_ROUNDTRIP => {
                    let id = r.u16() as u32;
                    let tag = r.u32();
                    self.send(EVENT_ROUNDTRIP_NOTIFY, &[id, tag])?;
                }
                // An operation this client does not know: the rest of the message cannot be parsed.
                _ => return Ok(()),
            }
        }
        Ok(())
    }
}

/// Little-endian fields of a server message; reads past the end yield zeros.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        if let Some(bytes) = self.data.get(self.pos..self.pos + N) {
            out.copy_from_slice(bytes);
        }
        self.pos += N;
        out
    }

    fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take())
    }

    fn i16(&mut self) -> i32 {
        i16::from_le_bytes(self.take()) as i32
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }
}

//! PTY + terminal durumu: alacritty_terminal ile VT ayrıştırma, portable-pty ile kabuk süreci.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Column;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Terminal boyutu (satır/sütun).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
    fn last_column(&self) -> Column {
        Column(self.cols.saturating_sub(1))
    }
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// Terminalden gelen olayları dinler: başlık değişimi, PTY'ye yazılması gereken yanıtlar, yeniden çizim.
#[derive(Clone)]
pub struct Proxy {
    ctx: egui::Context,
    writer: Writer,
    title: Arc<Mutex<Option<String>>>,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => self.ctx.request_repaint(),
            Event::Title(t) => *self.title.lock().unwrap() = Some(t),
            Event::ResetTitle => *self.title.lock().unwrap() = None,
            Event::PtyWrite(s) => {
                if let Ok(mut w) = self.writer.lock() {
                    let _ = w.write_all(s.as_bytes());
                }
            }
            _ => {}
        }
    }
}

/// Bir sekmeye karşılık gelen kabuk oturumu.
pub struct Session {
    pub term: Arc<Mutex<Term<Proxy>>>,
    writer: Writer,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Box<dyn Child + Send + Sync>,
    title: Arc<Mutex<Option<String>>>,
    size: Mutex<Size>,
}

impl Session {
    pub fn spawn(ctx: egui::Context, size: Size, banner: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let pty = native_pty_system();
        let pair = pty.openpty(PtySize {
            rows: size.rows as u16,
            cols: size.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Zenith");
        if let Some(home) = std::env::var_os("HOME") {
            cmd.cwd(home);
        }
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let title = Arc::new(Mutex::new(None));

        let proxy = Proxy { ctx: ctx.clone(), writer: writer.clone(), title: title.clone() };
        let config = Config { scrolling_history: 10_000, ..Default::default() };
        let mut t = Term::new(config, &size, proxy);
        if !banner.is_empty() {
            let mut p: Processor = Processor::new();
            for &b in banner.as_bytes() { p.advance(&mut t, b); }
        }
        let term = Arc::new(Mutex::new(t));

        // Okuma iş parçacığı: PTY çıktısını ayrıştırıcıya besler.
        let term_bg = term.clone();
        let ctx_bg = ctx.clone();
        thread::spawn(move || {
            let mut parser: Processor = Processor::new();
            let mut buf = [0u8; 65_536];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if let Ok(mut t) = term_bg.lock() {
                            for &b in &buf[..n] { parser.advance(&mut *t, b); }
                        }
                        ctx_bg.request_repaint();
                    }
                }
            }
            ctx_bg.request_repaint();
        });

        Ok(Self { term, writer, master: Mutex::new(pair.master), child, title, size: Mutex::new(size) })
    }

    pub fn write(&self, bytes: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    pub fn resize_shared(&self, size: Size) {
        if size.cols < 2 || size.rows < 1 {
            return;
        }
        {
            let mut cur = self.size.lock().unwrap();
            if *cur == size {
                return;
            }
            *cur = size;
        }
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(PtySize {
                rows: size.rows as u16,
                cols: size.cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        if let Ok(mut t) = self.term.lock() {
            t.resize(size);
        }
    }

    pub fn title(&self) -> Option<String> {
        self.title.lock().ok().and_then(|t| t.clone())
    }

    /// Kabuk süreci bitti mi?
    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

//! The system clipboard, behind a trait so tests never touch the real one.

/// What the TUI needs from a clipboard.
pub trait Clip: Send {
    fn set(&mut self, text: &str) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}

/// The real clipboard, connected on first use. The TUI empties it after 60 seconds and on exit,
/// but a clipboard manager (Klipper, GNOME history, ...) may have kept its own copy: turn its
/// history off, or paste the phrase somewhere safe and nowhere else.
#[derive(Default)]
pub struct SystemClip {
    inner: Option<arboard::Clipboard>,
}

impl SystemClip {
    fn open(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.inner.is_none() {
            self.inner = Some(
                arboard::Clipboard::new()
                    .map_err(|e| format!("no clipboard available here: {e}"))?,
            );
        }
        Ok(self.inner.as_mut().expect("just set"))
    }
}

impl Clip for SystemClip {
    fn set(&mut self, text: &str) -> Result<(), String> {
        self.open()?
            .set_text(text.to_owned())
            .map_err(|e| format!("could not copy: {e}"))
    }

    fn clear(&mut self) -> Result<(), String> {
        self.open()?
            .clear()
            .map_err(|e| format!("could not clear the clipboard: {e}"))
    }
}

//! The wallet dialog: pick, create, add or delete a wallet. Opened with `w` (Ctrl+W inside a
//! field). A new wallet gets a freshly generated recovery phrase, shown once.

use std::time::{Duration, Instant};

use crate::app::wallets::{self, DEFAULT_WORDS};
use crate::tuiapp::app::AppState;
use crate::tuiapp::send::Secret;

/// How long a recovery phrase stays on screen, and how long a copy stays in the clipboard.
pub const SECRET_LIFETIME: Duration = Duration::from_secs(60);

/// The word counts BIP39 allows, in the order Up/Down cycles through them.
const WORD_CHOICES: [usize; 5] = [24, 21, 18, 15, 12];

#[derive(Debug, Clone)]
pub enum ModalMode {
    List {
        selected: usize,
    },
    /// Name a new wallet whose recovery phrase will be generated.
    Create {
        name: String,
        words: usize,
    },
    /// The new wallet's phrase, until `shown_at` + 60 s or the user closes the dialog.
    Created {
        name: String,
        address: String,
        phrase: Option<Secret>,
        words: usize,
        shown_at: Instant,
        /// What the user has typed towards the word `saved`, the only way to dismiss the phrase.
        ack: String,
    },
    /// Watch an address that already exists (no phrase involved).
    New {
        name: String,
        address: String,
        on_address: bool,
    },
}

#[derive(Debug, Clone)]
pub struct WalletModal {
    pub mode: ModalMode,
    pub error: Option<String>,
    pub notice: Option<String>,
}

impl WalletModal {
    pub fn of(mode: ModalMode) -> Self {
        WalletModal {
            mode,
            error: None,
            notice: None,
        }
    }
    pub fn list(selected: usize) -> Self {
        Self::of(ModalMode::List { selected })
    }
    pub fn create(name: String) -> Self {
        Self::of(ModalMode::Create {
            name,
            words: DEFAULT_WORDS,
        })
    }
}

/// One key pressed while the dialog is open.
pub enum ModalKey {
    Char(char),
    Backspace,
    Enter,
    Up,
    Down,
    Tab,
    Clear,
    /// Esc: leave the dialog (or go back from a form to the list).
    Close,
}

fn cycle(words: usize, step: isize) -> usize {
    let i = WORD_CHOICES.iter().position(|w| *w == words).unwrap_or(0) as isize;
    let n = WORD_CHOICES.len() as isize;
    WORD_CHOICES[(i + step).rem_euclid(n) as usize]
}

impl AppState {
    pub fn open_wallets(&mut self) {
        let selected = self
            .wallets
            .as_ref()
            .and_then(|w| {
                let current = w.current()?;
                w.list().iter().position(|x| x.name == current.name)
            })
            .unwrap_or(0);
        let empty = self.wallets.as_ref().is_some_and(|w| w.list().is_empty());
        self.modal = Some(if empty {
            WalletModal::create(String::new())
        } else {
            WalletModal::list(selected)
        });
    }

    /// Copies the shown recovery phrase and starts the 60-second clipboard timer.
    fn copy_phrase(&mut self, shown: &WalletModal, now: Instant) -> Result<(), String> {
        let ModalMode::Created {
            phrase: Some(phrase),
            ..
        } = &shown.mode
        else {
            return Err("the phrase is no longer shown".into());
        };
        self.clip.set(phrase.expose())?;
        self.clip_expiry = Some(now + SECRET_LIFETIME);
        Ok(())
    }

    pub fn modal_key(&mut self, key: ModalKey) {
        self.modal_key_at(key, Instant::now());
    }

    pub fn modal_key_at(&mut self, key: ModalKey, now: Instant) {
        let Some(mut modal) = self.modal.take() else {
            return;
        };
        modal.error = None;
        let count = self.wallets.as_ref().map_or(0, |w| w.list().len());
        let mut keep = true;
        let mut copy = false;
        match (&mut modal.mode, key) {
            // The phrase is wiped as the dialog goes away.
            (
                ModalMode::Created {
                    phrase: Some(_), ..
                },
                ModalKey::Char('c' | 'C'),
            ) => copy = true,
            (
                ModalMode::Created {
                    phrase: Some(_),
                    ack,
                    ..
                },
                ModalKey::Char(c),
            ) => {
                if c.is_ascii_alphabetic() {
                    ack.push(c.to_ascii_lowercase());
                }
            }
            (
                ModalMode::Created {
                    phrase: Some(_),
                    ack,
                    ..
                },
                ModalKey::Backspace,
            ) => {
                ack.pop();
            }
            (
                ModalMode::Created {
                    phrase: Some(_),
                    ack,
                    ..
                },
                ModalKey::Clear,
            ) => ack.clear(),
            // One stray Enter or Esc must not throw the phrase away for good.
            (
                ModalMode::Created {
                    phrase: Some(_),
                    ack,
                    ..
                },
                ModalKey::Enter | ModalKey::Close,
            ) => {
                if ack == "saved" {
                    keep = false;
                } else {
                    modal.error = Some(
                        "The phrase cannot be shown again. Once you have written it down, type \
                         `saved` and press Enter."
                            .into(),
                    );
                }
            }
            // After the timer (or when there is nothing to protect) the dialog just closes.
            (ModalMode::Created { .. }, ModalKey::Enter | ModalKey::Close) => keep = false,

            (ModalMode::List { .. }, ModalKey::Close) => keep = false,
            (ModalMode::Create { .. } | ModalMode::New { .. }, ModalKey::Close) => {
                modal = WalletModal::list(0);
                if count == 0 {
                    keep = false;
                }
            }
            (ModalMode::List { selected }, ModalKey::Up) => *selected = selected.saturating_sub(1),
            (ModalMode::List { selected }, ModalKey::Down) => {
                *selected = (*selected + 1).min(count.saturating_sub(1))
            }
            (ModalMode::List { .. }, ModalKey::Char('q' | 'w')) => keep = false,
            (ModalMode::List { .. }, ModalKey::Char('n')) => {
                modal = WalletModal::create(String::new())
            }
            (ModalMode::List { .. }, ModalKey::Char('a')) => {
                modal.mode = ModalMode::New {
                    name: String::new(),
                    address: String::new(),
                    on_address: false,
                }
            }
            (ModalMode::List { selected }, ModalKey::Char('d')) => {
                let name = self
                    .wallets
                    .as_ref()
                    .and_then(|w| w.list().get(*selected).map(|x| x.name.clone()));
                if let (Some(name), Some(wallets)) = (name, self.wallets.as_mut()) {
                    match wallets.remove(&name) {
                        Ok(()) => {
                            *selected = (*selected).min(wallets.list().len().saturating_sub(1));
                            self.reset_send_flow();
                        }
                        Err(e) => modal.error = Some(e.to_string()),
                    }
                }
            }
            (ModalMode::List { selected }, ModalKey::Enter) => {
                let name = self
                    .wallets
                    .as_ref()
                    .and_then(|w| w.list().get(*selected).map(|x| x.name.clone()));
                match (name, self.wallets.as_mut()) {
                    (Some(name), Some(wallets)) => match wallets.select(&name) {
                        Ok(()) => {
                            self.reset_send_flow();
                            keep = false;
                        }
                        Err(e) => modal.error = Some(e.to_string()),
                    },
                    _ => keep = false,
                }
            }

            // Names are lowercase: capitals typed here become lowercase instead of an error.
            (ModalMode::Create { name, .. }, ModalKey::Char(c)) => {
                if !c.is_control() {
                    name.push(c.to_ascii_lowercase());
                }
            }
            (ModalMode::Create { name, .. }, ModalKey::Backspace) => {
                name.pop();
            }
            (ModalMode::Create { name, .. }, ModalKey::Clear) => name.clear(),
            (ModalMode::Create { words, .. }, ModalKey::Up) => *words = cycle(*words, -1),
            (ModalMode::Create { words, .. }, ModalKey::Down) => *words = cycle(*words, 1),
            (ModalMode::Create { name, .. }, ModalKey::Tab) => {
                let name = std::mem::take(name);
                modal.mode = ModalMode::New {
                    name,
                    address: String::new(),
                    on_address: true,
                };
            }
            (ModalMode::Create { name, words }, ModalKey::Enter) => {
                let (name, words) = (name.trim().to_owned(), *words);
                match self.create_wallet(&name, words) {
                    Ok(address) => {
                        // `create_wallet` left the phrase in `pending_phrase` for display.
                        let phrase = self.pending_phrase.take();
                        modal.mode = ModalMode::Created {
                            name,
                            address,
                            phrase,
                            words,
                            shown_at: now,
                            ack: String::new(),
                        };
                    }
                    Err(e) => modal.error = Some(e),
                }
            }

            (
                ModalMode::New {
                    name,
                    address,
                    on_address,
                },
                ModalKey::Char(c),
            ) => {
                if !c.is_control() {
                    // Addresses are case-sensitive; names are not, and are kept lowercase.
                    if *on_address {
                        address.push(c);
                    } else {
                        name.push(c.to_ascii_lowercase());
                    }
                }
            }
            (
                ModalMode::New {
                    name,
                    address,
                    on_address,
                },
                ModalKey::Backspace,
            ) => {
                if *on_address {
                    address.pop()
                } else {
                    name.pop()
                };
            }
            (
                ModalMode::New {
                    name,
                    address,
                    on_address,
                },
                ModalKey::Clear,
            ) => {
                if *on_address {
                    address.clear()
                } else {
                    name.clear()
                }
            }
            (ModalMode::New { on_address, .. }, ModalKey::Tab) => *on_address = !*on_address,
            (
                ModalMode::New {
                    name,
                    address,
                    on_address,
                },
                ModalKey::Enter,
            ) => {
                if !*on_address {
                    *on_address = true;
                } else if let Some(wallets) = self.wallets.as_mut() {
                    match wallets.add(name.trim(), address.trim()) {
                        Ok(()) => {
                            self.reset_send_flow();
                            keep = false;
                        }
                        Err(e) => modal.error = Some(e.to_string()),
                    }
                } else {
                    modal.error = Some("wallets are unavailable: no config directory".into());
                }
            }
            _ => {}
        }

        if copy {
            match self.copy_phrase(&modal, now) {
                Ok(()) => modal.notice = Some("Copied. The clipboard clears in 60 seconds.".into()),
                Err(e) => modal.error = Some(e),
            }
        }
        if keep {
            self.modal = Some(modal);
        }
    }

    /// Generates a phrase, saves the wallet (name + address only) and parks the phrase in
    /// `pending_phrase` for the dialog to show.
    fn create_wallet(&mut self, name: &str, words: usize) -> Result<String, String> {
        let network = self.network;
        let Some(store) = self.wallets.as_mut() else {
            return Err("wallets are unavailable: no config directory".into());
        };
        // Check the name first so a phrase is not generated for a wallet that cannot be saved.
        wallets::validate_name(name).map_err(|e| e.to_string())?;
        if store.list().iter().any(|w| w.name == name) {
            return Err(format!("a wallet named `{name}` exists"));
        }
        let new = wallets::generate(network, words).map_err(|e| e.to_string())?;
        store.add(name, &new.address).map_err(|e| e.to_string())?;
        let mut secret = Secret::default();
        for c in new.phrase.chars() {
            secret.push(c);
        }
        self.pending_phrase = Some(secret);
        self.reset_send_flow();
        Ok(new.address)
    }

    /// Run on every frame: hides an old phrase and clears an old copy.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now());
    }

    pub fn tick_at(&mut self, now: Instant) {
        if let Some(WalletModal {
            mode: ModalMode::Created {
                phrase, shown_at, ..
            },
            notice,
            ..
        }) = &mut self.modal
        {
            if phrase.is_some() && now.duration_since(*shown_at) >= SECRET_LIFETIME {
                *phrase = None;
                *notice = Some("The phrase was hidden after 60 seconds.".into());
            }
        }
        if self
            .status
            .as_ref()
            .is_some_and(|s| s.until.is_some_and(|at| now >= at))
        {
            self.status = None;
        }
        if self.clip_expiry.is_some_and(|at| now >= at) {
            self.clear_clipboard();
        }
    }

    /// Empties the clipboard if we put the phrase there.
    pub fn clear_clipboard(&mut self) {
        if self.clip_expiry.take().is_some() {
            let _ = self.clip.clear();
        }
    }
}

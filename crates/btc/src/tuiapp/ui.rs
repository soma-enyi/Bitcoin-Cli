use crate::tuiapp::app::{AppState, OperationState, Tab, TabState};
use crate::tuiapp::send::{FIELD_AMOUNT, FIELD_FEE, FIELD_TO, SendForm, SendStage};
use crate::tuiapp::wallet_modal::{ModalMode, SECRET_LIFETIME, WalletModal};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Tabs, Wrap};

const LOADING_SPINNERS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(frame: &mut Frame, app: &AppState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_content(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);
    if let Some(modal) = &app.modal {
        draw_wallet_modal(frame, app, modal);
    }
}

fn draw_header(frame: &mut Frame, app: &AppState, area: Rect) {
    let tabs: Vec<&str> = Tab::all().iter().map(|t| t.name()).collect();

    let current = Tab::all()
        .iter()
        .position(|t| t == &app.current_tab)
        .unwrap_or(0);

    let tab_widget = Tabs::new(tabs)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .style(Style::default().fg(Color::Cyan))
                .border_type(ratatui::widgets::BorderType::Rounded),
        )
        .select(current)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tab_widget, area);
}

fn draw_content(frame: &mut Frame, app: &AppState, area: Rect) {
    match &app.tab_state {
        TabState::KeysAndMnemonic { .. } => draw_keys_mnemonic(frame, app, area),
        TabState::Derive { .. } => draw_derive(frame, app, area),
        TabState::Addresses { .. } => draw_addresses(frame, app, area),
        TabState::TxDecoder { .. } => draw_tx_decoder(frame, app, area),
        TabState::BlockExplorer { .. } => draw_block_explorer(frame, app, area),
        TabState::Fees { .. } => draw_fees(frame, app, area),
        TabState::Send { .. } => draw_send(frame, app, area),
        TabState::NodeStatus { .. } => draw_node_status(frame, app, area),
    }
}

fn draw_footer(frame: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let help = "⌨ ← →:Tabs │ Enter:Run │ w:Wallets │ m/Ctrl+O:Mine (regtest) │ Ctrl+U:Clear │ q/Esc:Save & quit";
    let help_widget = Paragraph::new(help)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::DarkGray))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(help_widget, chunks[0]);

    // A message (mining result, ...) takes the place of the usual line while it lasts.
    let (status, color) = match &app.status {
        Some(s) => (
            s.text.clone(),
            if s.is_error { Color::Red } else { Color::Green },
        ),
        None => (
            format!(
                "Net: {} | Wallet: {} | Tab: {}",
                app.network_label,
                app.wallet_label(),
                app.current_tab.name()
            ),
            Color::Cyan,
        ),
    };
    let status_widget = Paragraph::new(status)
        .alignment(Alignment::Right)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD));

    frame.render_widget(status_widget, chunks[1]);
}

fn get_spinner_frame(frame_count: u32) -> &'static str {
    let index = (frame_count / 5) as usize % LOADING_SPINNERS.len();
    LOADING_SPINNERS[index]
}

fn draw_state_indicator(state: &OperationState, frame_count: u32) -> (String, Color) {
    match state {
        OperationState::Idle => ("✓ Ready".to_string(), Color::Green),
        OperationState::Loading => {
            let spinner = get_spinner_frame(frame_count);
            (format!("{} Processing...", spinner), Color::Yellow)
        }
        OperationState::Success(_) => ("✓ Success".to_string(), Color::Green),
        OperationState::Error(_) => ("✗ Error".to_string(), Color::Red),
    }
}

fn draw_keys_mnemonic(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::KeysAndMnemonic {
        key_type_selected,
        state,
    } = &app.tab_state
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 🔐 Key & Mnemonic Generator ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let input_text = vec![
            Line::from(""),
            Line::from(vec![
                "Mode: ".into(),
                if *key_type_selected {
                    "📝 Mnemonic"
                } else {
                    "🔑 Key Pair"
                }
                .to_string()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            ]),
            Line::from(""),
            Line::from("Press SPACE to toggle mode"),
            Line::from("Press ENTER to generate"),
        ];

        let p = Paragraph::new(input_text).block(Block::default());
        frame.render_widget(p, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Ready to generate... Press Enter")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::DarkGray)),
                    );
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading_text = vec![
                    Line::from(""),
                    Line::from(format!(
                        "{} Generating cryptographic material...",
                        get_spinner_frame(app.frame_count)
                    ))
                    .style(Style::default().fg(Color::Yellow)),
                ];
                let loading = Paragraph::new(loading_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Yellow)),
                    )
                    .alignment(Alignment::Center);
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(output) => {
                let output_text = Paragraph::new(output.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Green))
                            .title(" ✓ Output "),
                    );
                frame.render_widget(output_text, chunks[1]);
            }
            OperationState::Error(err) => {
                let error_text = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Red))
                            .title(" ✗ Error "),
                    );
                frame.render_widget(error_text, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_derive(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::Derive {
        xprv_input,
        path_input,
        count,
        state,
    } = &app.tab_state
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 📈 HD Wallet Derivation ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let input_box = Paragraph::new(format!(
            "xprv Key:      {}\nPath:          {}\nKey Count:     {}",
            if xprv_input.is_empty() {
                "[empty]"
            } else {
                xprv_input
            },
            if path_input.is_empty() {
                "[empty]"
            } else {
                path_input
            },
            count
        ))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Enter xprv key and press Enter to derive...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::DarkGray)),
                    );
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!(
                    "{} Deriving keys from path...",
                    get_spinner_frame(app.frame_count)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Green))
                            .title(" ✓ Derived Keys "),
                    );
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Red))
                            .title(" ✗ Error "),
                    );
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_addresses(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::Addresses {
        pubkey_input,
        addr_type,
        state,
    } = &app.tab_state
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 🏘️  Address Generator ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let addr_types = ["P2PKH", "P2WPKH", "P2SH", "P2WSH"];
        let input_box = Paragraph::new(format!(
            "Public Key:    {}\nAddress Type:  {} (0:P2PKH, 1:P2WPKH, 2:P2SH, 3:P2WSH)",
            if pubkey_input.is_empty() {
                "[empty]"
            } else {
                pubkey_input
            },
            addr_types.get(*addr_type).unwrap_or(&"Unknown")
        ))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Enter public key and press Enter...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::DarkGray)),
                    );
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!(
                    "{} Generating address...",
                    get_spinner_frame(app.frame_count)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Green))
                            .title(" ✓ Address "),
                    );
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Red))
                            .title(" ✗ Error "),
                    );
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_tx_decoder(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::TxDecoder { hex_input, state } = &app.tab_state {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 🔍 Transaction Decoder ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let input_box = Paragraph::new(format!(
            "Tx:  {}",
            if hex_input.is_empty() {
                "[raw hex, a txid, or txid@height]"
            } else {
                hex_input
            }
        ))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new(
                    "Paste raw hex, a txid, or <txid>@<block height> and press Enter...",
                )
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::DarkGray)),
                );
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!(
                    "{} Decoding transaction...",
                    get_spinner_frame(app.frame_count)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Green))
                            .title(" ✓ Tx Details "),
                    );
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Red))
                            .title(" ✗ Error "),
                    );
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_block_explorer(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::BlockExplorer {
        height_input,
        state,
    } = &app.tab_state
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" ⛓️  Block Explorer ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let input_box = Paragraph::new(format!(
            "Height:  {}",
            if height_input.is_empty() {
                "[enter block height]"
            } else {
                height_input
            }
        ))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Enter block height and press Enter...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::DarkGray)),
                    );
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!(
                    "{} Fetching block data...",
                    get_spinner_frame(app.frame_count)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Green))
                            .title(" ✓ Block Info "),
                    );
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Red))
                            .title(" ✗ Error "),
                    );
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_fees(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::Fees {
        target_blocks,
        height_input,
        blocks_input,
        blocks_focused,
        state,
    } = &app.tab_state
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 💰 Fee Estimator ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let gauge = Gauge::default()
            .block(Block::default())
            .gauge_style(Style::default().fg(Color::Cyan))
            .percent((target_blocks.min(&100) * 100 / 100) as u16)
            .label(format!("{} blocks", target_blocks));

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);
        frame.render_widget(gauge, rows[0]);

        let field = |label: &str, value: &str, hint: &str, focused: bool| {
            let marker = if focused { "▶" } else { " " };
            let shown = if value.is_empty() { hint } else { value };
            let color = if focused {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            Paragraph::new(format!("{marker} {label}: {shown}")).style(Style::default().fg(color))
        };
        frame.render_widget(
            field(
                "Last block ",
                height_input,
                "[optional height; blank = node estimator]",
                !*blocks_focused,
            ),
            rows[1],
        );
        frame.render_widget(
            field(
                "Blocks read",
                blocks_input,
                &format!(
                    "[optional; blank = {} for this target]",
                    btc_node::fee_source::default_window(*target_blocks as u16)
                ),
                *blocks_focused,
            ),
            rows[2],
        );

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new(
                    "↑↓ target · Tab switch field · type a last block / number of blocks (optional) · Enter estimates",
                )
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::DarkGray)),
                    );
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!(
                    "{} Estimating fees...",
                    get_spinner_frame(app.frame_count)
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Green))
                            .title(" ✓ Fee Estimate "),
                    );
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Red))
                            .title(" ✗ Error "),
                    );
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_node_status(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::NodeStatus { state } = &app.tab_state {
        let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 🕸️  Bitcoin Node Status ")
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Press Enter to check node status...")
                    .style(Style::default().fg(Color::Yellow))
                    .alignment(Alignment::Center);
                frame.render_widget(empty, inner);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!(
                    "{} Connecting to Bitcoin Core...",
                    get_spinner_frame(app.frame_count)
                ))
                .style(Style::default().fg(Color::Yellow))
                .alignment(Alignment::Center);
                frame.render_widget(loading, inner);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green));
                frame.render_widget(output, inner);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red));
                frame.render_widget(error, inner);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[1]);
    }
}

fn draw_send(frame: &mut Frame, app: &AppState, area: Rect) {
    let TabState::Send { form, state } = &app.tab_state else {
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    let (status_text, status_color) = draw_state_indicator(state, app.frame_count);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 💸 Send ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan))
        .border_type(ratatui::widgets::BorderType::Rounded);
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);
    frame.render_widget(Paragraph::new(send_form_lines(app, form)), inner);

    let (text, color, title) = match state {
        OperationState::Loading => (
            format!(
                "{} Working... scanning the node for coins can take a minute on mainnet.",
                get_spinner_frame(app.frame_count)
            ),
            Color::Yellow,
            " … ",
        ),
        OperationState::Error(e) => (e.clone(), Color::Red, " ✗ Error "),
        OperationState::Success(t) => (t.clone(), Color::Green, " ✓ Result "),
        OperationState::Idle if !form.summary.is_empty() => {
            (form.summary.clone(), Color::Green, " Payment ")
        }
        OperationState::Idle => (
            "Fill in the form, then Enter. Tab/↑↓ move between fields. Ctrl+W: wallets.".into(),
            Color::DarkGray,
            "",
        ),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(color))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::default().fg(color))
                    .title(title),
            ),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center),
        chunks[2],
    );
}

fn send_form_lines<'a>(app: &AppState, form: &'a SendForm) -> Vec<Line<'a>> {
    let wallet = match app.wallets.as_ref().and_then(|w| w.current()) {
        Some(w) => format!("Wallet: {} ({})", w.name, w.address),
        None => "No wallet yet: press Ctrl+W to create one".to_owned(),
    };
    let dim = Style::default().fg(Color::DarkGray);
    let hot = Style::default().fg(Color::Yellow);
    let mut lines = vec![Line::styled(wallet, Style::default().fg(Color::Cyan))];
    match &form.stage {
        SendStage::Compose => {
            let field = |i: usize, label: &str, value: &str, hint: &str| {
                let focused = form.focus == i;
                let shown = if value.is_empty() { hint } else { value };
                Line::styled(
                    format!("{} {label}: {shown}", if focused { "▶" } else { " " }),
                    if focused { hot } else { dim },
                )
            };
            lines.push(field(FIELD_TO, "Pay to    ", &form.to, "[address]"));
            lines.push(field(
                FIELD_AMOUNT,
                "Amount sat",
                &form.amount,
                "[e.g. 25000]",
            ));
            lines.push(field(
                FIELD_FEE,
                "Fee sat/vB",
                &form.fee,
                "[blank = ask the node]",
            ));
            lines.push(Line::styled("Enter: build the payment", dim));
        }
        SendStage::Review { .. } => {
            lines.push(Line::styled("Check the payment below.", hot));
            lines.push(Line::styled("Enter: sign   b: edit", dim));
        }
        SendStage::Key { .. } => {
            lines.push(Line::styled(
                format!(
                    "Private key (WIF) or recovery phrase: {}",
                    "•".repeat(form.key.len())
                ),
                hot,
            ));
            lines.push(Line::styled(
                "Typed here only to sign: it is never saved. Enter: sign   Tab: back",
                dim,
            ));
        }
        SendStage::Confirm { txid, .. } => {
            lines.push(Line::styled(format!("Signed: {txid}"), hot));
            if app.mainnet {
                lines.push(Line::styled(
                    format!("MAINNET: type yes to broadcast: {}", form.confirm),
                    Style::default().fg(Color::Red),
                ));
            }
            lines.push(Line::styled("Enter: broadcast   Tab: discard", dim));
        }
        SendStage::Done => lines.push(Line::styled("Sent. Enter: a new payment", hot)),
    }
    lines
}

fn draw_wallet_modal(frame: &mut Frame, app: &AppState, modal: &WalletModal) {
    use ratatui::widgets::Clear;
    let area = frame.area();
    let width = area.width.min(78);
    let height = area.height.min(20);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);

    let mut lines: Vec<Line> = Vec::new();
    let title = match &modal.mode {
        ModalMode::List { selected } => {
            let current = app.wallets.as_ref().and_then(|w| w.current());
            for (i, w) in app.wallets.iter().flat_map(|w| w.list()).enumerate() {
                let marker = if i == *selected { "▶" } else { " " };
                let active = if current.is_some_and(|c| c.name == w.name) {
                    " ✓"
                } else {
                    ""
                };
                lines.push(Line::styled(
                    format!("{marker} {}{active}  {}", w.name, w.address),
                    if i == *selected {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "↑↓ pick · Enter use · n create new · a add an address · d delete · Esc close",
                Style::default().fg(Color::DarkGray),
            ));
            " Wallets "
        }
        ModalMode::Create { name, words } => {
            lines.push(Line::styled(
                format!("▶ Name: {name}"),
                Style::default().fg(Color::Yellow),
            ));
            lines.push(Line::styled(
                format!("  Recovery phrase length: {words} words (↑↓ to change)"),
                Style::default().fg(Color::Gray),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "A new phrase is generated for you. Enter create · Tab use an existing address · Esc back",
                Style::default().fg(Color::DarkGray),
            ));
            " Create wallet "
        }
        ModalMode::Created {
            name,
            address,
            phrase,
            words,
            shown_at,
            ack,
        } => {
            lines.push(Line::styled(
                format!("Wallet `{name}` created: {address}"),
                Style::default().fg(Color::Green),
            ));
            // 24 words need 6 rows plus the header, hint and borders.
            let fits = frame.area().height >= 20;
            match phrase {
                Some(_) if !fits => {
                    lines.push(Line::styled(
                        "The window is too short to show the recovery phrase: make it at least \
                         20 rows tall. The phrase is hidden after 60 seconds.",
                        Style::default().fg(Color::Red),
                    ));
                }
                Some(phrase) => {
                    let left = SECRET_LIFETIME.saturating_sub(shown_at.elapsed()).as_secs();
                    lines.push(Line::styled(
                        format!("Write these {words} words down, in order. Hidden in {left}s."),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ));
                    for (row, chunk) in phrase
                        .expose()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .chunks(4)
                        .enumerate()
                    {
                        let cells: Vec<String> = chunk
                            .iter()
                            .enumerate()
                            .map(|(i, w)| format!("{:>2} {:<9}", row * 4 + i + 1, w))
                            .collect();
                        lines.push(Line::styled(
                            cells.join(" "),
                            Style::default().fg(Color::Yellow),
                        ));
                    }
                    lines.push(Line::styled(
                        "c copy (clipboard clears after 60s)",
                        Style::default().fg(Color::DarkGray),
                    ));
                    lines.push(Line::styled(
                        format!("Written down? Type saved + Enter to close for good: {ack}"),
                        Style::default().fg(Color::Yellow),
                    ));
                }
                None => {
                    lines.push(Line::styled(
                        "The phrase is hidden. If you did not write it down, delete this wallet \
                         (d in the list) and create another. Enter: close",
                        Style::default().fg(Color::Yellow),
                    ));
                }
            }
            " Recovery phrase "
        }
        ModalMode::New {
            name,
            address,
            on_address,
        } => {
            let row = |label: &str, value: &str, focused: bool| {
                Line::styled(
                    format!("{} {label}: {value}", if focused { "▶" } else { " " }),
                    if focused {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                )
            };
            lines.push(row("Name   ", name, !*on_address));
            lines.push(row("Address", address, *on_address));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Only the name and address are saved, never a key. Tab switch · Enter next/save · Esc back",
                Style::default().fg(Color::DarkGray),
            ));
            " New wallet "
        }
    };
    if let Some(n) = &modal.notice {
        lines.push(Line::raw(""));
        lines.push(Line::styled(n.clone(), Style::default().fg(Color::Green)));
    }
    if let Some(e) = &modal.error {
        lines.push(Line::raw(""));
        lines.push(Line::styled(e.clone(), Style::default().fg(Color::Red)));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().fg(Color::Cyan))
                .border_type(ratatui::widgets::BorderType::Rounded),
        ),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::tuiapp::app::Tab;

    fn screen(app: &AppState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .chunks(width as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_send_tab_and_wallet_dialog_draw_at_normal_and_tiny_sizes() {
        let mut app = AppState::new();
        app.go_to(Tab::Send);
        let text = screen(&app, 100, 30);
        assert!(
            text.contains("Pay to") && text.contains("No wallet yet"),
            "{text}"
        );

        app.open_wallets();
        let text = screen(&app, 100, 30);
        assert!(text.contains("Wallets"), "{text}");
        app.modal = Some(WalletModal::create("carol".into()));
        let text = screen(&app, 100, 30);
        assert!(
            text.contains("Create wallet") && text.contains("24 words"),
            "{text}"
        );
        let mut secret = crate::tuiapp::send::Secret::default();
        for c in "abandon ability able about".chars() {
            secret.push(c);
        }
        app.modal = Some(WalletModal::of(ModalMode::Created {
            name: "carol".into(),
            address: "bcrt1qexample".into(),
            phrase: Some(secret),
            words: 4,
            shown_at: std::time::Instant::now(),
            ack: String::new(),
        }));
        let text = screen(&app, 100, 30);
        assert!(
            text.contains("abandon") && text.contains("Hidden in"),
            "{text}"
        );
        // Too short to show 24 words: the words are withheld rather than clipped.
        let short = screen(&app, 100, 15);
        assert!(
            !short.contains("abandon") && short.contains("too short"),
            "{short}"
        );
        app.modal = None;
        app.status = Some(crate::tuiapp::app::Status {
            text: "Mined 101 blocks to `alice`".into(),
            is_error: false,
            until: None,
        });
        let text = screen(&app, 120, 30);
        assert!(
            text.contains("Mined 101 blocks") && text.contains("Ctrl+O:Mine"),
            "{text}"
        );
        screen(&app, 30, 8);
        // A cramped terminal must not panic.
        screen(&app, 20, 5);
        app.modal = None;
        screen(&app, 20, 5);
        app.go_to(Tab::Fees);
        let text = screen(&app, 100, 30);
        assert!(text.contains("Blocks read"), "{text}");
    }
}

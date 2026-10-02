use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Tabs, Wrap};
use crate::tuiapp::app::{AppState, Tab, TabState, OperationState};

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
}

fn draw_header(frame: &mut Frame, app: &AppState, area: Rect) {
    let tabs: Vec<&str> = Tab::all()
        .iter()
        .map(|t| t.name())
        .collect();

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
        TabState::NodeStatus { .. } => draw_node_status(frame, app, area),
    }
}

fn draw_footer(frame: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let help = "⌨ ← →:Tabs │ Enter:Run │ Ctrl+U:Clear field │ Esc:Quit";
    let help_widget = Paragraph::new(help)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP).style(Style::default().fg(Color::Cyan)));

    frame.render_widget(help_widget, chunks[0]);

    let status = format!("Net: {} | Tab: {}", app.network_label, app.current_tab.name());
    let status_widget = Paragraph::new(status)
        .alignment(Alignment::Right)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

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
                if *key_type_selected { "📝 Mnemonic" } else { "🔑 Key Pair" }
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
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading_text = vec![
                    Line::from(""),
                    Line::from(format!("{} Generating cryptographic material...", get_spinner_frame(app.frame_count)))
                        .style(Style::default().fg(Color::Yellow)),
                ];
                let loading = Paragraph::new(loading_text)
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center);
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(output) => {
                let output_text = Paragraph::new(output.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Green)).title(" ✓ Output "));
                frame.render_widget(output_text, chunks[1]);
            }
            OperationState::Error(err) => {
                let error_text = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)).title(" ✗ Error "));
                frame.render_widget(error_text, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
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
            .constraints([Constraint::Length(8), Constraint::Min(3), Constraint::Length(2)])
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
            if xprv_input.is_empty() { "[empty]" } else { xprv_input },
            if path_input.is_empty() { "[empty]" } else { path_input },
            count
        ))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Enter xprv key and press Enter to derive...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!("{} Deriving keys from path...", get_spinner_frame(app.frame_count)))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Green)).title(" ✓ Derived Keys "));
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)).title(" ✗ Error "));
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
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
            .constraints([Constraint::Length(7), Constraint::Min(3), Constraint::Length(2)])
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
            if pubkey_input.is_empty() { "[empty]" } else { pubkey_input },
            addr_types.get(*addr_type).unwrap_or(&"Unknown")
        ))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Enter public key and press Enter...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!("{} Generating address...", get_spinner_frame(app.frame_count)))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Green)).title(" ✓ Address "));
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)).title(" ✗ Error "));
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_tx_decoder(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::TxDecoder { hex_input, state } = &app.tab_state {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(3), Constraint::Length(2)])
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

        let input_box = Paragraph::new(format!("Tx:  {}", if hex_input.is_empty() { "[raw hex, a txid, or txid@height]" } else { hex_input }))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Paste raw hex, a txid, or <txid>@<block height> and press Enter...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!("{} Decoding transaction...", get_spinner_frame(app.frame_count)))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Green)).title(" ✓ Tx Details "));
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)).title(" ✗ Error "));
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_block_explorer(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::BlockExplorer { height_input, state } = &app.tab_state {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(3), Constraint::Length(2)])
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

        let input_box = Paragraph::new(format!("Height:  {}", if height_input.is_empty() { "[enter block height]" } else { height_input }))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Yellow));

        frame.render_widget(input_box, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Enter block height and press Enter...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!("{} Fetching block data...", get_spinner_frame(app.frame_count)))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Green)).title(" ✓ Block Info "));
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)).title(" ✗ Error "));
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn draw_fees(frame: &mut Frame, app: &AppState, area: Rect) {
    if let TabState::Fees { target_blocks, state } = &app.tab_state {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(3), Constraint::Length(2)])
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

        frame.render_widget(gauge, inner);

        match state {
            OperationState::Idle => {
                let empty = Paragraph::new("Adjust blocks with ↑↓, press Enter to estimate...")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(empty, chunks[1]);
            }
            OperationState::Loading => {
                let loading = Paragraph::new(format!("{} Estimating fees...", get_spinner_frame(app.frame_count)))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow));
                frame.render_widget(loading, chunks[1]);
            }
            OperationState::Success(out) => {
                let output = Paragraph::new(out.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Green)).title(" ✓ Fee Estimate "));
                frame.render_widget(output, chunks[1]);
            }
            OperationState::Error(err) => {
                let error = Paragraph::new(err.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)).title(" ✗ Error "));
                frame.render_widget(error, chunks[1]);
            }
        }

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
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
                let loading = Paragraph::new(format!("{} Connecting to Bitcoin Core...", get_spinner_frame(app.frame_count)))
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
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(status_bar, chunks[1]);
    }
}

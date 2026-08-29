//! Portable single-executable entry point (`docs/SPEC_CS.md` §22).
//!
//! The profile — identity, settings, history — lives beside the binary so the
//! whole directory can be copied to another machine. Nothing is written to the
//! registry or outside this folder.
//!
//! ```text
//! p2p-poker                      the client, with its window
//! p2p-poker --headless           the node only, printing what happens
//! p2p-poker --headless --host N  and offering a table called N
//! p2p-poker --for 120            stop after 120 seconds, for a scripted run
//! p2p-poker --table              open on the table rather than the lobby
//! ```
//!
//! The headless mode is not a lesser client. It is what a scripted two-machine
//! test drives and what a volunteer relay runs, and it prints the same events
//! the window shows.

use std::time::Duration;

use p2p_poker::app::AppState;
use p2p_poker::gui::{render, table};
use p2p_poker::net::node::NodeEvent;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    println!("p2p-poker {}", env!("CARGO_PKG_VERSION"));

    let dir = p2p_poker::storage::profile::profile_dir();
    let identity = match p2p_poker::storage::profile::load_or_create_identity(&dir) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("profile at {}: {e}", dir.display());
            return;
        }
    };
    println!("peer id  {}", libp2p::PeerId::from(identity.public()));

    let hosted = value_of("--host").map(|name| {
        // Seeded from this crate's one randomness source. `ed25519-dalek` 3.0
        // wants `rand_core` 0.10's trait and our handle speaks `rand` 0.8's, and
        // threading a second generator in to bridge that is the exact thing
        // `security::rng` exists to prevent.
        let seed = p2p_poker::security::rng::secret_32()
            .expect("the operating system CSPRNG is available");
        println!("hosting  {name}");
        p2p_poker::net::run::Hosted {
            ad: demo_table(name),
            key: ed25519_dalek::SigningKey::from_bytes(&seed),
        }
    });

    let bounded = value_of("--for").and_then(|v| v.parse::<u64>().ok());

    if has("--headless") {
        headless(identity, hosted, bounded);
    } else {
        windowed(
            identity,
            hosted,
            bounded,
            if has("--table") {
                Screen::Table
            } else {
                Screen::Lobby
            },
        );
    }
}

/// The node, printing what happens. No window.
fn headless(
    identity: libp2p::identity::Keypair,
    hosted: Option<p2p_poker::net::run::Hosted>,
    bounded: Option<u64>,
) {
    let rt = tokio::runtime::Runtime::new().expect("a tokio runtime");
    rt.block_on(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            if let Err(e) = p2p_poker::net::run::run(identity, tx, hosted).await {
                eprintln!("node stopped: {e}");
            }
        });

        let mut state = AppState::new();
        let deadline = async {
            match bounded {
                Some(secs) => tokio::time::sleep(Duration::from_secs(secs)).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::pin!(deadline);

        println!(
            "running  {}",
            bounded
                .map(|s| format!("{s} s"))
                .unwrap_or_else(|| "until Ctrl-C".into())
        );

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                _ = &mut deadline => break,
                Some(event) = rx.recv() => {
                    // Folded through the same state the window uses, so the two
                    // modes cannot disagree about what happened.
                    state.apply(event);
                    if let Some(line) = state.log.back() {
                        println!("{line}");
                    }
                }
            }
        }
        println!("done");
    });
}

/// The client, with its window.
fn windowed(
    identity: libp2p::identity::Keypair,
    hosted: Option<p2p_poker::net::run::Hosted>,
    bounded: Option<u64>,
    screen: Screen,
) {
    let rt = tokio::runtime::Runtime::new().expect("a tokio runtime");
    let (tx, rx) = tokio::sync::mpsc::channel(256);

    // The node runs on the tokio runtime and the window on this thread. They
    // share a channel and nothing else, which is what keeps `SPEC_CS.md` §33
    // true: a 95 ms shuffle proof on the paint thread is six dropped frames.
    rt.spawn(async move {
        if let Err(e) = p2p_poker::net::run::run(identity, tx, hosted).await {
            eprintln!("node stopped: {e}");
        }
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("p2p-poker"),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    let result = eframe::run_native(
        "p2p-poker",
        options,
        Box::new(move |cc| {
            render::install(&cc.egui_ctx);
            Ok(Box::new(Client {
                state: AppState::new(),
                screen,
                ui: Default::default(),
                table_ui: Default::default(),
                events: rx,
                bounded,
                started,
                _rt: rt,
            }))
        }),
    );
    if let Err(e) = result {
        eprintln!("the window could not open: {e}");
        eprintln!("run with --headless if this machine has no display");
    }
}

/// Which of the two windows the one window is showing.
///
/// One viewport rather than two: a second operating-system window is a second
/// thing to lose behind the first, and the table has a way back to the lobby on
/// it. Multi-tabling will want real windows and will get them then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Lobby,
    Table,
}

struct Client {
    state: AppState,
    screen: Screen,
    /// What the user is typing, which must survive the snapshot being replaced.
    ui: p2p_poker::gui::render::LobbyUi,
    table_ui: p2p_poker::gui::table::TableUi,
    events: tokio::sync::mpsc::Receiver<NodeEvent>,
    bounded: Option<u64>,
    started: std::time::Instant,
    /// Kept alive: dropping the runtime would stop the node.
    _rt: tokio::runtime::Runtime,
}

impl eframe::App for Client {
    // eframe 0.36 hands the root viewport's `Ui` directly rather than a
    // `Context`; the panel is already open by the time this is called.
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Drain what has arrived, bounded per frame so a burst cannot stall the
        // paint loop — which is the same rule as everywhere else: anything fed
        // from the network is bounded where it is consumed.
        for _ in 0..64 {
            match self.events.try_recv() {
                Ok(event) => self.state.apply(event),
                Err(_) => break,
            }
        }

        if let Some(secs) = self.bounded {
            if self.started.elapsed() >= Duration::from_secs(secs) {
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
        }

        match self.screen {
            Screen::Lobby => {
                let view = self.state.view();
                match render::lobby(ui, &view, &mut self.ui) {
                    render::LobbyAction::Select(key) => self.state.selected = Some(key),
                    render::LobbyAction::None => {}
                    render::LobbyAction::OpenTableWindow => self.screen = Screen::Table,
                    // The rest is not wired to the transport yet, and saying so
                    // is better than a button that appears to work.
                    other => self
                        .state
                        .log
                        .push_back(format!("{other:?} is not wired to the transport yet")),
                }
            }
            Screen::Table => {
                // No hand can be in progress until formation and the engine are
                // wired, so the table shows a sample and says that it is one.
                // §22 forbids passing an unverified card off as a real one, and
                // a preview that admits what it is does not.
                let view = table::TableView::sample();
                match table::draw(ui, &view, &mut self.table_ui) {
                    table::TableAction::BackToLobby => self.screen = Screen::Lobby,
                    table::TableAction::None => {}
                    other => self
                        .state
                        .log
                        .push_back(format!("{other:?} is not wired to the engine yet")),
                }
            }
        }

        // The node pushes events whether or not the window is being interacted
        // with, so the window is repainted on a timer rather than only on input.
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

/// A `CUSTOM` six-seat table, with a deadline derived from its own shape.
///
/// `CUSTOM` and not a name of its own: §7.2 rule 3 admits exactly two preset
/// identifiers, and a name that asserts values nothing checks is how two clients
/// ship different tables under one identity.
fn demo_table(name: String) -> p2p_poker::net::lobby::TableAd {
    use p2p_poker::net::lobby::{BlindSchedule, TableAd, DECK_SUITE_V1};
    use p2p_poker::protocol::constants::hand_deadline_min_ms;

    let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
    let seats = 6u8;
    TableAd {
        game: 1,
        mode: 1,
        preset_id: "CUSTOM".into(),
        table_name: name,
        small_blind: 10,
        big_blind: 20,
        ante: 0,
        min_buyin: 200,
        max_buyin: 2_000,
        start_stack: 0,
        players: 1,
        max_players: seats,
        min_players_to_start: 2,
        blind_schedule: BlindSchedule {
            mode: 1,
            every_n_hands: 20,
            first_small_blind: 10,
            small_blind_cap: 1_000,
        },
        action_timeout_ms: action,
        action_grace_ms: grace,
        crypto_step_timeout_ms: crypto,
        hand_deadline_ms: hand_deadline_min_ms(
            seats,
            action as u64,
            grace as u64,
            crypto as u64,
            delay as u64,
        ) as u32,
        join_deadline_ms: 120_000,
        hand_delay_ms: delay,
        button_rule: 1,
        odd_chip_rule: 1,
        showdown_policy: 1,
        password_required: false,
        deck_suite: DECK_SUITE_V1.into(),
        founder_app_key: [0u8; 32],
        founder_peer_id: Vec::new(),
        timestamp_unix_ms: 0,
        expires_at_unix_ms: 0,
    }
}

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
//! p2p-poker --headless --join N  sit down at the first table called N
//! p2p-poker --profile DIR        keep the profile somewhere other than beside
//!                                the binary
//! ```
//!
//! `--profile` exists because two clients on one machine must be two players.
//! The profile lives beside the executable so the whole folder can be copied,
//! and two copies of one folder are one identity — which §4.3's `peer_id` rule
//! then correctly refuses a second seat to. Pointing the second instance at its
//! own directory is what makes a two-instance test a test of two players rather
//! than of one player joining twice.
//!
//! The headless mode is not a lesser client. It is what a scripted two-machine
//! test drives and what a volunteer relay runs, and it prints the same events
//! the window shows.

use std::time::Duration;

use p2p_poker::app::AppState;
use p2p_poker::gui::{render, table};
use p2p_poker::net::node::{NodeCommand, NodeEvent};

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

    let dir = value_of("--profile")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(p2p_poker::storage::profile::profile_dir);
    let identity = match p2p_poker::storage::profile::load_or_create_identity(&dir) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("profile at {}: {e}", dir.display());
            return;
        }
    };
    println!("peer id  {}", libp2p::PeerId::from(identity.public()));

    // The **player's** identity, which is not the network's. §20 keeps the two
    // apart: a peer id says which socket you are talking to, this says who is
    // playing, and it is what appears in every roster this client ever joins.
    let app_key = match p2p_poker::storage::profile::load_or_create_app_key(&dir) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("player key at {}: {e}", dir.display());
            return;
        }
    };
    println!(
        "player   {}",
        app_key.verifying_key().to_bytes()[..4]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );

    // Hosting is a command like any other, taken by the same path the button
    // takes. It used to be a second construction here, with its own table
    // advertisement built by hand — two ways to start a table is two places for
    // the advertisement and the roster to disagree about what table it is.
    let hosted = value_of("--host").map(|name| {
        println!("hosting  {name}");
        NodeCommand::CreateTable {
            name,
            seats: 6,
            min_players: 2,
            buyin: 1_000,
            password: None,
        }
    });

    let bounded = value_of("--for").and_then(|v| v.parse::<u64>().ok());

    if has("--headless") {
        headless(identity, app_key, hosted, bounded, value_of("--join"));
    } else {
        windowed(
            identity,
            app_key,
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
    app_key: ed25519_dalek::SigningKey,
    hosted: Option<NodeCommand>,
    bounded: Option<u64>,
    join: Option<String>,
) {
    let rt = tokio::runtime::Runtime::new().expect("a tokio runtime");
    rt.block_on(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        // The receiving end must outlive the loop whether or not anything is
        // ever sent: an `mpsc::Receiver` whose senders are all gone completes
        // immediately and for ever, and its `select!` arm would spin.
        let (commands, command_rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            if let Err(e) = p2p_poker::net::run::run(identity, app_key, tx, command_rx).await {
                eprintln!("node stopped: {e}");
            }
        });
        if let Some(command) = hosted {
            let _ = commands.send(command).await;
        }

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
                    let seen = matches!(event, NodeEvent::TableSeen { .. });
                    state.apply(event);
                    if let Some(line) = state.log.back() {
                        println!("{line}");
                    }

                    // A table this run was told to sit down at, recognised by
                    // the name in its advertisement. The name is **display data
                    // and never an identifier** (§4.3) — two tables may share
                    // one, and this takes the first that arrives. That is fine
                    // for a scripted run and would not be fine in a client,
                    // which is why it is only here.
                    //
                    // Checked **after** the fold, not before: the store is what
                    // the fold fills, and the first version asked it a question
                    // one event too early and never joined anything.
                    if let (Some(want), true, None) = (&join, seen, state.seated.as_ref()) {
                        let found = state
                            .lobby
                            .tables()
                            .find(|l| &l.held.ad.table_name == want)
                            .map(|l| (*l.key, l.held.ad.max_buyin));
                        if let Some((key, buyin)) = found {
                            println!("asking to join {want}");
                            let _ = commands
                                .send(NodeCommand::JoinTable {
                                    key,
                                    buyin,
                                    seat: None,
                                    password: None,
                                })
                                .await;
                        }
                    }
                }
            }
        }
        // The one line a scripted run reads back. A table with a session is a
        // table that formed; anything else is not, and saying which is the whole
        // point of running two of these.
        match state.seated.as_ref().and_then(|s| s.session) {
            Some(session) => println!(
                "TABLE FORMED session={} seats={}",
                session[..8].iter().map(|b| format!("{b:02x}")).collect::<String>(),
                state.seated.as_ref().map(|s| s.roster.len()).unwrap_or(0)
            ),
            None => println!("NO TABLE"),
        }
        println!("done");
    });
}

/// The client, with its window.
fn windowed(
    identity: libp2p::identity::Keypair,
    app_key: ed25519_dalek::SigningKey,
    hosted: Option<NodeCommand>,
    bounded: Option<u64>,
    screen: Screen,
) {
    let rt = tokio::runtime::Runtime::new().expect("a tokio runtime");
    let (tx, rx) = tokio::sync::mpsc::channel(256);
    // The other direction. Bounded, and the window never blocks on it: a full
    // queue means the node is busy, and a paint loop that waited for it would
    // freeze the client rather than drop a button press.
    let (commands, command_rx) = tokio::sync::mpsc::channel(16);

    // The node runs on the tokio runtime and the window on this thread. They
    // share a channel and nothing else, which is what keeps `SPEC_CS.md` §33
    // true: a 95 ms shuffle proof on the paint thread is six dropped frames.
    let opening = commands.clone();
    rt.spawn(async move {
        if let Some(command) = hosted {
            let _ = opening.send(command).await;
        }
        if let Err(e) = p2p_poker::net::run::run(identity, app_key, tx, command_rx).await {
            eprintln!("node stopped: {e}");
        }
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            // Three panes and an eight-column list. The first sizes were 980
            // by 700 and 720 by 480, and at both of them the table list ran off
            // its own column: the minimum in particular was a size at which the
            // client could not show what it is for.
            .with_inner_size([1_180.0, 760.0])
            .with_min_inner_size([900.0, 600.0])
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
                commands,
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
    commands: tokio::sync::mpsc::Sender<NodeCommand>,
    bounded: Option<u64>,
    started: std::time::Instant,
    /// Kept alive: dropping the runtime would stop the node.
    _rt: tokio::runtime::Runtime,
}

impl Client {
    /// Hand a command to the node, without ever waiting for it.
    ///
    /// `try_send` and not `send`: this runs on the paint thread, and a paint
    /// thread that blocks on a channel is a frozen window. A full queue means
    /// the node is busy, which is worth saying and is not worth stopping for.
    fn tell(&mut self, command: NodeCommand) {
        if self.commands.try_send(command).is_err() {
            self.state
                .log
                .push_back("the node is busy; try that again".into());
        }
    }
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
                    render::LobbyAction::Create(t) => self.tell(NodeCommand::CreateTable {
                        name: t.name,
                        seats: t.seats,
                        min_players: t.min_players,
                        buyin: t.buyin,
                        password: if t.password.is_empty() {
                            None
                        } else {
                            Some(t.password.into_bytes())
                        },
                    }),
                    render::LobbyAction::Sit {
                        key,
                        buyin,
                        password,
                    } => self.tell(NodeCommand::JoinTable {
                        key,
                        buyin,
                        seat: None,
                        password,
                    }),
                    render::LobbyAction::LeaveTable => self.tell(NodeCommand::LeaveTable),
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

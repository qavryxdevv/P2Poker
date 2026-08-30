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
//! p2p-poker --host N --seats 6   a six-handed Sit-and-Go
//! p2p-poker --host N --cash      a cash table, which deals with two
//! p2p-poker --renderer software  draw without a graphics driver
//! p2p-poker --no-mdns             do not look for players by multicast
//! p2p-poker --port 4242           listen on a fixed port, to forward on a router
//! ```
//!
//! `--renderer` is there to be overridden, not to be typed. The client draws
//! with OpenGL, and a machine with no graphics driver — a virtual machine
//! without acceleration, most often — has only the OpenGL 1.1 Windows ships,
//! where the window needs 2.0. Rather than fail, the client starts itself again
//! on Direct3D 12, which falls through to WARP, the software rasteriser Windows
//! itself carries. Slow, and it needs nothing installed. `--renderer gl` or
//! `--renderer software` pins the choice and skips the second attempt.
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
use p2p_poker::gui::lobby::short_key;
use p2p_poker::gui::render;
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

    // libp2p says a great deal through `tracing` and, without a subscriber,
    // says it to nobody. Every network question asked of this client so far has
    // been answered by adding a temporary `eprintln!` and rebuilding, which is
    // slow and leaves nothing behind. Off unless `RUST_LOG` is set, so it costs
    // an environment lookup at start-up and nothing else.
    //
    //     RUST_LOG=libp2p_kad=debug,libp2p_relay=debug p2p-poker --headless
    if std::env::var_os("RUST_LOG").is_some() {
        use tracing_subscriber::{fmt, EnvFilter};
        let _ = fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
    }

    println!("p2p-poker {}", env!("CARGO_PKG_VERSION"));

    let dir = value_of("--profile")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(p2p_poker::storage::profile::profile_dir);
    let identity = match p2p_poker::storage::profile::load_or_create_identity(&dir) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("profile at {}: {e}", dir.display());
            // Not `return`. Without an identity there is no client, and a
            // scripted run - `check-portable.ps1`, `deploy.ps1`'s proof that
            // the copy runs - reads the exit code and would have recorded a
            // pass.
            std::process::exit(1);
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
            std::process::exit(1);
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
        use p2p_poker::net::lobby::TableKind;
        use p2p_poker::protocol::constants::{RATED_START_STACK, RATED_SEATS};
        // A Sit-and-Go by default, ten-handed and therefore rated — the same
        // table the button founds. `--seats` sizes it; `--cash` makes it a cash
        // table instead, which is what a two-process test wants because a
        // Sit-and-Go deals only when every seat is full.
        let seats = value_of("--seats")
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(RATED_SEATS)
            .clamp(2, RATED_SEATS);
        println!("hosting  {name}");
        NodeCommand::CreateTable {
            kind: if has("--cash") {
                TableKind::Cash
            } else {
                TableKind::SitAndGo
            },
            name,
            seats,
            min_players: value_of("--min")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(2)
                .clamp(2, seats),
            buyin: RATED_START_STACK,
            password: None,
        }
    });

    let settings = p2p_poker::storage::settings::load(&dir, &app_key);
    println!("name     {}", settings.nickname);

    let bounded = value_of("--for").and_then(|v| v.parse::<u64>().ok());

    // Multicast discovery, on unless refused. `--no-mdns` exists to prove the
    // other path: with it on, two clients on one wire find each other in under
    // a second whatever the DHT does, so a run that means to test the DHT has
    // to take it away first.
    let local_discovery = !has("--no-mdns");

    // A fixed port, for a player who can forward one.
    //
    // Zero means "whatever the OS gives", which is right for somebody behind a
    // NAT they do not control and useless for somebody who can open a door:
    // a forwarding rule names a number, and an ephemeral one is different every
    // start. Naming it here makes that player reachable, which AutoNAT can then
    // confirm — and a confirmed player becomes a relay for everybody else.
    let port = value_of("--port")
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(0);

    // Parsed before the headless branch, so a value nobody understands is
    // refused whether or not a window was going to open. A flag that is
    // silently ignored in one mode is a flag somebody will trust in the other.
    let asked = value_of("--renderer");
    let draw = match asked.as_deref() {
        None | Some("auto") => Draw::Gl,
        Some("gl") => Draw::Gl,
        Some("software") => Draw::Software,
        Some(other) => {
            eprintln!("--renderer takes auto, gl or software, not {other:?}");
            std::process::exit(2);
        }
    };

    let player = Player {
        identity,
        app_key,
        profile_dir: dir,
        settings,
    };
    let run = Run {
        hosted,
        bounded,
        screen: if has("--table") {
            Screen::Table
        } else {
            Screen::Lobby
        },
        draw,
        local_discovery,
        port,
    };

    if has("--headless") {
        headless(player, run, value_of("--join"));
        return;
    }

    let outcome = windowed(player, run);

    // Every local `windowed` held is dropped by now — the tokio runtime with the
    // node on it included. That ordering is the whole reason this decision is
    // taken out here rather than at the point of failure: a second process would
    // otherwise start a second node under the same identity, on ports the first
    // one had not let go of yet. §4.3 gives one seat per `peer_id`, so two live
    // copies of one profile is not a slow client, it is a refused seat.
    if outcome == Started::NoOpenGl {
        if asked.is_none() {
            println!();
            println!("No OpenGL 2.0 on this machine. Starting again in software.");
            std::process::exit(again_in_software(&args));
        }
        // Pinned by hand, so no second attempt is made — but the advice is
        // still owed. Without this the person who typed `--renderer gl` got one
        // line of glutin's own words and nothing else.
        advice(Draw::Gl);
        std::process::exit(1);
    }
    if outcome != Started::Ok {
        std::process::exit(1);
    }
}

/// The child's arguments: ours, with the renderer settled.
///
/// Separate from the spawn so it can be tested, because the one thing that must
/// not go wrong here is a child that reads `auto` and starts a third process.
/// Any `--renderer` the user gave is dropped **with its value** - dropping the
/// flag alone would leave a bare `auto` sitting where `--profile` expects a
/// directory.
fn software_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() + 1);
    let mut rest = args.iter().skip(1);
    while let Some(a) = rest.next() {
        if a == "--renderer" {
            rest.next();
            continue;
        }
        out.push(a.clone());
    }
    out.push("--renderer".to_owned());
    out.push("software".to_owned());
    out
}

/// How many node events one frame will apply before it gives up and asks for
/// another.
///
/// A bound rather than "drain it all", because everything fed from the network
/// is bounded where it is consumed, and a peer that could make this window
/// spend an unbounded amount of time before painting is a peer that can freeze
/// the client.
const EVENTS_PER_FRAME: usize = 64;

/// How long a log line may wait before the window is redrawn for it.
///
/// Measured, on this machine, with a full-size lobby and nothing happening:
///
/// | renderer | one repaint | idle cost at the old 4 Hz |
/// |---|---|---|
/// | OpenGL, on a card | ~4 ms | 1.4% of one core |
/// | Direct3D 12 on WARP | ~500 ms | 660% — 6.6 cores |
///
/// The window is the same, and so is the work egui does to build it: measured
/// from inside, the client's own `ui` takes 0.6 ms of that. What differs is the
/// rasteriser, and a processor shading 900 000 pixels is three orders of
/// magnitude off a graphics card doing the same.
///
/// So the machine that can afford to be prompt is prompt, and the one that
/// cannot batches its chatter. Three seconds is not a compromise on anything a
/// player watches: `NodeEvent::changes_more_than_the_log` sends everything of
/// that kind down the immediate path.
fn quiet_wake(draw: Draw) -> Duration {
    match draw {
        Draw::Gl => Duration::from_millis(200),
        Draw::Software => Duration::from_secs(3),
    }
}

/// Which of the two renderers draws the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Draw {
    /// OpenGL through the graphics driver: every machine that has one.
    Gl,
    /// Direct3D 12, which on a machine with no graphics driver resolves to
    /// WARP — `Microsoft Basic Render Driver`, a `Cpu` adapter that is part of
    /// Windows rather than of any driver, and so is present in a bare virtual
    /// machine. Measured on a developer box: it is enumerated alongside the two
    /// real GPUs, version 10.0.19041, which is the Windows build, not a driver.
    Software,
}

/// How far the window got.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Started {
    /// It opened, ran, and closed.
    Ok,
    /// The renderer wanted OpenGL 2.0 and this machine has 1.1 — worth a second
    /// attempt in software, which is the one case the client can fix by itself.
    NoOpenGl,
    /// Anything else: no display at all, or software failed too.
    Failed,
}

/// Start this same executable again, drawing in software, and wait for it.
///
/// A second process because a process gets one event loop and no more:
/// `winit` swaps a global flag the first time one is built and never clears it,
/// so a renderer cannot be retried in place — `EventLoopError::RecreationAttempt`
/// is all a second attempt would produce.
///
/// Every flag the user gave is carried over, minus any `--renderer` of their
/// own, so the child cannot read `auto` and start a third process.
fn again_in_software(args: &[String]) -> i32 {
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("cannot find this executable to start it again");
        return 1;
    };
    let mut command = std::process::Command::new(exe);
    command.args(software_args(args));
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("could not start again in software: {e}");
            1
        }
    }
}

/// The node, printing what happens. No window.
fn headless(player: Player, run: Run, join: Option<String>) {
    let Player {
        identity,
        app_key,
        settings,
        profile_dir,
    } = player;
    let Run {
        hosted,
        bounded,
        local_discovery,
        port,
        ..
    } = run;
    let rt = tokio::runtime::Runtime::new().expect("a tokio runtime");
    rt.block_on(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        // The receiving end must outlive the loop whether or not anything is
        // ever sent: an `mpsc::Receiver` whose senders are all gone completes
        // immediately and for ever, and its `select!` arm would spin.
        let (commands, command_rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            if let Err(e) = p2p_poker::net::run::run(identity, app_key, tx, command_rx, local_discovery, port, profile_dir).await {
                eprintln!("node stopped: {e}");
            }
        });
        // The name first, so a table founded a moment later carries it.
        let _ = commands
            .send(NodeCommand::SetNickname(settings.nickname.clone()))
            .await;
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
                    // What the log had before, so only new lines are printed.
                    // Most events add none — a re-broadcast this client already
                    // holds, a peer count — and printing `log.back()` after
                    // every one of them reprinted the previous line instead,
                    // which read as the same thing happening five times.
                    let before = state.log.len();
                    state.apply(event);
                    for line in state.log.iter().skip(before) {
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
/// Who is playing: everything read out of the profile directory.
struct Player {
    identity: libp2p::identity::Keypair,
    app_key: ed25519_dalek::SigningKey,
    profile_dir: std::path::PathBuf,
    settings: p2p_poker::storage::settings::Settings,
}

/// What this particular start is for: everything that came off the command line.
struct Run {
    hosted: Option<NodeCommand>,
    bounded: Option<u64>,
    screen: Screen,
    draw: Draw,
    local_discovery: bool,
    port: u16,
}

fn windowed(player: Player, run: Run) -> Started {
    let Player {
        identity,
        app_key,
        profile_dir,
        settings,
    } = player;
    let Run {
        hosted,
        bounded,
        screen,
        draw,
        local_discovery,
        port,
    } = run;
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
    let opening_name = settings.nickname.clone();
    // The window keeps a copy: it saves the settings, and the defaults a
    // settings file falls back to are derived from this key.
    let node_key = app_key.clone();
    // The node writes its peer book here; the window writes the settings. Two
    // owners of one path, which is a clone rather than an argument thread.
    let node_dir = profile_dir.clone();
    rt.spawn(async move {
        let _ = opening.send(NodeCommand::SetNickname(opening_name)).await;
        if let Some(command) = hosted {
            let _ = opening.send(command).await;
        }
        if let Err(e) = p2p_poker::net::run::run(identity, node_key, tx, command_rx, local_discovery, port, node_dir).await {
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
            .with_title("p2p-poker")
            .with_icon(window_icon()),
        renderer: match draw {
            Draw::Gl => eframe::Renderer::Glow,
            Draw::Software => eframe::Renderer::Wgpu,
        },
        wgpu_options: software_wgpu(),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    let result = eframe::run_native(
        "p2p-poker",
        options,
        Box::new(move |cc| {
            // The first thing said from inside a window that exists. `eframe`
            // calls this only after the renderer has a surface, so a script has
            // something to wait for — and the software path's `drawing …` line
            // is printed while choosing an adapter, which is earlier and proves
            // less.
            println!(
                "window   open ({})",
                match draw {
                    Draw::Gl => "gl",
                    Draw::Software => "software",
                }
            );
            render::install(&cc.egui_ctx);

            // Wake the window when the node speaks, rather than asking it to
            // look. What was here was `request_repaint_after(250 ms)` at the
            // end of every frame, unconditionally: the window painted four
            // times a second whether or not anything had changed, purely to
            // reach the `try_recv` at the top of `ui`.
            //
            // On a graphics card that costs 1.4% of one core and nobody
            // notices. On the software renderer a frame takes **longer than the
            // interval**, so the next paint is due before the last one is
            // finished and it never stops: 660% of a core - 6.6 of them -
            // sitting at a table with nothing happening. Both measured.
            //
            // A relay rather than a shared buffer, so the boundedness survives:
            // this second channel is the same size as the first, back-pressure
            // reaches the node exactly as before, and the order events arrive
            // in is the order they are applied - which `AppState` depends on.
            let (woken, events) = tokio::sync::mpsc::channel(256);
            let waker = cc.egui_ctx.clone();
            let mut arriving = rx;
            let lazily = quiet_wake(draw);
            rt.spawn(async move {
                while let Some(event) = arriving.recv().await {
                    // Asked before the event is handed over, because after it is
                    // sent it belongs to the window.
                    //
                    // Both wakes name `ViewportId::ROOT`. A bare
                    // `request_repaint` wakes "the current viewport", which is
                    // the top of a stack this task is not on; while the table's
                    // own window is open that is the table, and eframe checks a
                    // wake against the viewport it names before honouring it.
                    // With no timer left there is no second chance.
                    let now = event.changes_more_than_the_log();
                    if woken.send(event).await.is_err() {
                        break;
                    }
                    if now {
                        waker.request_repaint_of(eframe::egui::ViewportId::ROOT);
                    } else {
                        // Not "later" but "no sooner than": egui keeps the
                        // earliest outstanding request, so a seat filling in the
                        // meantime still repaints at once and this one rides
                        // along with it.
                        waker.request_repaint_after_for(lazily, eframe::egui::ViewportId::ROOT);
                    }
                }
            });

            let mut state = AppState::new();
            state.me = settings.nickname.clone();
            cc.egui_ctx.set_zoom_factor(settings.zoom());
            Ok(Box::new(Client {
                state,
                screen,
                table_closed: false,
                ui: render::LobbyUi::new(settings),
                table_ui: Default::default(),
                profile_dir,
                app_key,
                commands,
                events,
                bounded,
                started,
                _rt: rt,
            }))
        }),
    );
    match result {
        Ok(()) => Started::Ok,
        Err(e) => {
            let reason = e.to_string();
            // Returned rather than acted on. The caller is the one place where
            // this runtime and its node are certainly gone, which is what a
            // second process needs to be true before it starts.
            if draw == Draw::Gl && is_a_driver_problem(&e) {
                eprintln!("the window could not open: {reason}");
                Started::NoOpenGl
            } else {
                explain_window_failure(&reason, draw);
                Started::Failed
            }
        }
    }
}

/// Whether a software renderer could repair this failure.
///
/// Three of `eframe::Error`'s variants are the OpenGL path failing and no more
/// than that: the painter refusing the version (`OpenGL`), glutin failing
/// (`Glutin`), and glutin finding no usable framebuffer configuration at all
/// (`NoGlutinConfigs`) — the last of which is what a machine with no driver
/// whatsoever tends to produce. Direct3D 12 does not care about any of them.
///
/// Narrow on purpose, and this is the half that matters: `Winit` and
/// `WinitEventLoop` mean there is no display to draw on, where a second
/// renderer would fail in the same way and slower; and `AppCreation` is a fault
/// in **this** program, which a second attempt would only repeat.
///
/// An earlier version read the message text, on the belief that `eframe`
/// offered no variant to match. It does — `lib.rs:526` — and a string match
/// would have missed `NoGlutinConfigs` entirely, because its `Display` never
/// says "OpenGL".
fn is_a_driver_problem(e: &eframe::Error) -> bool {
    matches!(
        e,
        eframe::Error::OpenGL(_) | eframe::Error::Glutin(_) | eframe::Error::NoGlutinConfigs(..)
    )
}

/// How the software renderer picks its adapter.
///
/// Left to itself, wgpu asks for the *best* adapter and would take a real GPU
/// where one exists. Here the request is the opposite: this process only exists
/// because OpenGL was missing, so the processor is the point. A `Cpu` adapter is
/// preferred and the rest are kept as a fallback, because a virtual machine with
/// a paravirtual Direct3D 12 adapter and no OpenGL is a real configuration and
/// drawing on it beats not starting.
fn software_wgpu() -> eframe::egui_wgpu::WgpuConfiguration {
    // `eframe::wgpu`, not `wgpu`. The direct dependency exists only to choose
    // the backend set and is declared for Windows alone, so naming it here
    // would stop this file compiling anywhere else. `eframe` re-exports the
    // same crate at `lib.rs:162`.
    use eframe::wgpu;

    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.power_preference = wgpu::PowerPreference::LowPower;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, surface| {
        let usable = |a: &wgpu::Adapter| surface.is_none_or(|s| a.is_surface_supported(s));
        let picked = adapters
            .iter()
            .find(|a| a.get_info().device_type == wgpu::DeviceType::Cpu && usable(a))
            .or_else(|| adapters.iter().find(|a| usable(a)))
            .cloned()
            .ok_or_else(|| "no Direct3D 12 adapter at all, not even the software one".to_owned())?;
        // Printed, not logged. This line is the answer to "did the fallback
        // work?", and it is wanted by somebody staring at a terminal in a
        // virtual machine, who has no logger configured and should not need one.
        let info = picked.get_info();
        println!("drawing  {} ({:?})", info.name, info.device_type);
        Ok(picked)
    }));
    eframe::egui_wgpu::WgpuConfiguration {
        wgpu_setup: setup.into(),
        ..Default::default()
    }
}

/// Say why the window did not open, and what is left to try.
///
/// The first version said one thing for every failure: *"run with --headless if
/// this machine has no display"*. In a virtual machine that is the wrong advice
/// about the wrong problem - there **is** a display, and what is missing is a
/// graphics driver. Somebody following it would conclude their VM has no screen.
///
/// By the time this is reached the client has already tried the one repair it
/// can make on its own, so what is printed here is what is genuinely left.
fn explain_window_failure(reason: &str, draw: Draw) {
    eprintln!("the window could not open: {reason}");
    advice(draw);
}

/// What is left to try, given which renderer has just failed.
fn advice(draw: Draw) {
    eprintln!();
    match draw {
        Draw::Software => {
            eprintln!("This was the software renderer, so there is no Direct3D 12 here");
            eprintln!("either - not even WARP, which every Windows 10 carries. Either this");
            eprintln!("machine is older than that, or it has no display at all.");
            eprintln!();
            eprintln!("  p2p-poker --headless");
            eprintln!();
            eprintln!("runs the node with no window. It plays no poker, but it is a full peer");
            eprintln!("and a relay for others, and --host and --join work, which is what a");
            eprintln!("two-machine test needs.");
        }
        Draw::Gl => {
            eprintln!("  p2p-poker --renderer software   draw on the processor instead");
            eprintln!("  p2p-poker --headless            no window at all: node and relay only");
        }
    }
}

/// The icon both windows wear.
///
/// Decoded from the same file the executable's own icon is compiled from, so
/// the taskbar, the title bar and the file in Explorer cannot show three
/// different pictures. Decoded once per window rather than cached: it happens
/// twice in the life of the process.
///
/// A failure yields no icon rather than no client. The picture is not worth
/// refusing to start over.
fn window_icon() -> std::sync::Arc<eframe::egui::IconData> {
    const PNG: &[u8] = include_bytes!("../assets/icon-256.png");
    let empty = || {
        std::sync::Arc::new(eframe::egui::IconData {
            rgba: Vec::new(),
            width: 0,
            height: 0,
        })
    };
    match image::load_from_memory(PNG) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            std::sync::Arc::new(eframe::egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            })
        }
        Err(_) => empty(),
    }
}

/// Which screen the client opened on.
///
/// The table is its **own operating-system window**, beside the lobby rather
/// than instead of it — a player watching seats fill up wants to see the lobby
/// at the same time, and switching between them was the first version and was
/// wrong. `--table` opens it at start-up; otherwise it opens by itself the
/// moment this client has a seat somewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Lobby,
    Table,
}

struct Client {
    state: AppState,
    screen: Screen,
    /// The player shut the table window. It does not re-open by itself until
    /// they sit down somewhere else, or ask for it.
    table_closed: bool,
    /// Where the settings are saved, and the key their defaults come from.
    profile_dir: std::path::PathBuf,
    app_key: ed25519_dalek::SigningKey,
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
    /// The table, in its own operating-system window.
    ///
    /// `show_viewport_immediate` and not the deferred form: the deferred one
    /// wants a `'static` closure and this needs `&mut self`, which is the whole
    /// state the table is drawn from. Immediate costs a nested paint of this
    /// window inside the parent's frame, which for one table is nothing.
    ///
    /// Closing it does not leave the table — a player who shuts the window is
    /// still in the hand, and unseating them because they wanted the screen back
    /// would be the worst possible reading of a click. **Leave table** does
    /// that, in the lobby, deliberately.
    /// Apply what the node has said, up to a bound.
    ///
    /// Returns whether the queue was emptied. `false` means the bound stopped
    /// it, and the caller owes the window another frame.
    fn drain(&mut self) -> bool {
        for _ in 0..EVENTS_PER_FRAME {
            match self.events.try_recv() {
                Ok(event) => self.state.apply(event),
                Err(_) => return true,
            }
        }
        false
    }

    fn table_window(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui::{ViewportBuilder, ViewportId};

        let title = self
            .state
            .seated
            .as_ref()
            .map(|s| format!("p2p-poker — table {}", short_key(&s.key)))
            .unwrap_or_else(|| "p2p-poker — table".into());

        let view = self.table_view();
        let mut action = p2p_poker::gui::table::TableAction::None;
        let mut closed = false;

        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("p2p-poker-table"),
            ViewportBuilder::default()
                .with_title(title)
                .with_icon(window_icon())
                .with_inner_size([1_000.0, 720.0])
                .with_min_inner_size([760.0, 560.0]),
            |ctx, _class| {
                // `EmbeddedWindow` means the platform gave us a panel inside the
                // lobby rather than a window of its own. The table is drawn
                // either way — half of what was asked for beats none — and the
                // difference is only whether it has a frame of its own.
                eframe::egui::CentralPanel::default()
                    .frame(eframe::egui::Frame::NONE)
                    .show(ctx, |ui| {
                        action = p2p_poker::gui::table::draw(ui, &view, &mut self.table_ui);
                    });
                if ctx.input(|i| i.viewport().close_requested()) {
                    closed = true;
                }
            },
        );

        if closed || action == p2p_poker::gui::table::TableAction::BackToLobby {
            self.screen = Screen::Lobby;
            self.table_closed = true;
        }
        // The button, straight to the engine. The action is checked by this
        // client's own `BettingRound` before anything is sealed and by every
        // receiver against its own afterwards, and the two are the same code —
        // so a button that offered something illegal fails here rather than on
        // the wire.
        use p2p_poker::gui::table::TableAction as Ta;
        use p2p_poker::poker::actions::Action;
        let played = match action {
            Ta::Fold => Some(Action::Fold),
            Ta::Check => Some(Action::Check),
            Ta::Call => Some(Action::Call),
            // The bar hands back a **total for the round**, which is what the
            // wire carries and what the engine takes. Whether that total opens
            // the betting or raises it is the engine's to know, not the
            // window's, and asking the window would be a second opinion that
            // can disagree.
            Ta::Raise(total) => Some(match self.state.hand.as_ref().and_then(|h| h.turn) {
                Some(t) if t.can_bet => Action::Bet(total),
                _ => Action::Raise(total),
            }),
            Ta::None | Ta::BackToLobby => None,
        };
        if let Some(a) = played {
            self.tell(p2p_poker::net::node::NodeCommand::Act(a));
        }
    }

    /// What the table window draws.
    ///
    /// The **real** roster while this client has a seat, so a player watches the
    /// others arrive; the sample only when there is no table at all and the
    /// player asked for a look. §22's rule is kept by construction either way —
    /// a seated view has no cards in it, because no hand has been dealt.
    fn table_view(&self) -> p2p_poker::gui::table::TableView {
        use p2p_poker::gui::table::{SeatView, TableView};

        let Some(seat) = self.state.seated.as_ref() else {
            return TableView::sample();
        };

        // From the node, which knows them, and not from this client's own lobby
        // — a founder's table is not in its own lobby until the network has
        // taken the advertisement, and a table window showing invented defaults
        // for those thirty seconds is worse than one showing nothing.
        let name = if seat.name.is_empty() {
            format!("table {}", short_key(&seat.key))
        } else {
            seat.name.clone()
        };
        let blinds = format!("{} / {}", seat.small_blind, seat.big_blind);
        let max_seats = seat.seats.max(1);
        let needed = seat.needed;

        let seats = seat
            .roster
            .iter()
            .map(|(n, who, stack)| SeatView {
                seat: *n,
                name: who.clone(),
                stack: *stack,
                cards: hole_cards(self.state.hand.as_ref(), *n, seat.seat),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        let seated = seats.len();
        // The hand, if one has begun. Until `HAND_INIT` completes there is no
        // hand number, no button and nothing to say beyond who is still being
        // waited for — and saying *that* is the point: "no hand in progress"
        // told a player nothing they could act on.
        let hand = self.state.hand.as_ref();
        TableView {
            name,
            blinds,
            can_act: hand.and_then(|h| h.turn).is_some(),
            to_call: hand.and_then(|h| h.turn).map(|t| t.to_call).unwrap_or(0),
            min_raise: hand
                .and_then(|h| h.turn)
                .filter(|t| t.can_bet || t.can_raise)
                .map(|t| t.min_raise_to)
                .unwrap_or(0),
            max_raise: hand
                .and_then(|h| h.turn)
                .filter(|t| t.can_bet || t.can_raise)
                .map(|t| t.max_raise_to)
                .unwrap_or(0),
            pot: hand.and_then(|h| h.turn).map(|t| t.pot).unwrap_or(0),
            to_act: hand.and_then(|h| {
                h.turn
                    .map(|_| seat.seat.unwrap_or(0))
                    .or(h.waiting_on)
            }),
            board: board_cards(hand),
            street: match (hand, seat.session.is_some()) {
                (Some(h), _) if h.cards.is_some() => "pre-flop".into(),
                (Some(h), _) if h.deck_ready => "deck sealed".into(),
                (Some(_), _) => "shuffling".into(),
                (None, true) => "ready".into(),
                (None, false) => "waiting".into(),
            },
            hand: hand.map(|h| h.hand_id).unwrap_or(0),
            button: hand.map(|h| h.button).unwrap_or(0),
            seats,
            hero: seat.seat.unwrap_or(0),
            max_seats,
            note: Some(match (hand, seat.session) {
                // While the chain runs, whose turn it is says more than the
                // button does: it is the one thing on this screen that can be
                // late, and a player who knows which seat everybody is waiting
                // for knows whether the wait is theirs to fix.
                (Some(h), _) if h.cards.is_some() => format!(
                    "hand #{} — your cards are dealt; the button is at seat {}",
                    h.hand_id, h.button
                ),
                (Some(h), _) => match (h.shuffling, h.deck_ready) {
                    (_, true) => format!(
                        "hand #{} — the deck is shuffled and sealed; the button is at seat {}",
                        h.hand_id, h.button
                    ),
                    (Some(s), _) => format!(
                        "hand #{} — seat {s} is shuffling the deck",
                        h.hand_id
                    ),
                    (None, false) => format!(
                        "hand #{} — preparing the deck",
                        h.hand_id
                    ),
                },
                (None, Some(_)) if !self.state.waiting_for.is_empty() => {
                    let who: Vec<String> = self
                        .state
                        .waiting_for
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    format!("waiting for seat {} to open the hand", who.join(", "))
                }
                (None, Some(_)) => "everybody has ratified the roster; the table is set".into(),
                (None, None) => format!("waiting for players — {seated} of {needed}"),
            }),
            ..Default::default()
        }
    }

    fn tell(&mut self, command: NodeCommand) {
        if self.commands.try_send(command).is_err() {
            self.state
                .log
                .push_back("the node is busy; try that again".into());
        }
    }
}

/// The five board slots.
///
/// A card is face-up only where one has actually been opened — three after the
/// flop, four after the turn, five after the river — and `Facing::Empty` for
/// the rest. There is no back on the board: an unopened board card is not a
/// card somebody is holding, it is a card that does not exist yet.
fn board_cards(
    hand: Option<&p2p_poker::app::HandInProgress>,
) -> [p2p_poker::gui::table::Facing; 5] {
    use p2p_poker::gui::table::Facing;
    let mut out = [Facing::Empty; 5];
    let Some(h) = hand else {
        return out;
    };
    for (slot, index) in out.iter_mut().zip(h.board.iter()) {
        if let Ok(card) = p2p_poker::poker::state::Card::from_index(*index) {
            *slot = Facing::up(card, true);
        }
    }
    out
}

/// What to draw in one seat's two card slots.
///
/// Face-up only for this client's own seat, and only from cards it opened
/// itself: `Facing::up` takes a verdict and the verdict here is that the cards
/// came out of a complete set of verified shares. Every other seat gets backs
/// once the deal is done — those cards exist, this client holds `m-1` shares
/// for each of them, and it is one share short of every one of them by design.
/// Before the deal there is no card anywhere, and §22 is kept by there being
/// nothing to draw rather than by a check.
fn hole_cards(
    hand: Option<&p2p_poker::app::HandInProgress>,
    seat: u8,
    hero: Option<u8>,
) -> [p2p_poker::gui::table::Facing; 2] {
    use p2p_poker::gui::table::Facing;

    let Some(h) = hand else {
        return [Facing::Empty, Facing::Empty];
    };
    if !h.holding.contains(&seat) {
        return [Facing::Empty, Facing::Empty];
    }
    match (h.cards, hero) {
        (Some(cards), Some(me)) if me == seat => cards.map(|index| {
            match p2p_poker::poker::state::Card::from_index(index) {
                Ok(card) => Facing::up(card, true),
                // A byte outside the deck cannot come from a card this client
                // opened, so this is unreachable — and it draws a back rather
                // than panicking, because a covered card is a correct thing to
                // draw and a crashed table is not.
                Err(_) => Facing::Down,
            }
        }),
        _ => [Facing::Down, Facing::Down],
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
        //
        // **And say so when the bound bites.** While a timer asked for a frame
        // four times a second this was harmless: whatever was left over was
        // picked up 250 ms later. With the timer gone the only thing that wakes
        // this window is an event arriving, and the events that were left in the
        // queue have already been counted as arrived — so a burst of more than
        // 64 would sit there, unread, until something else happened. A hand
        // beginning inside such a burst would simply not be drawn.
        if !self.drain() {
            ctx.request_repaint();
        }

        if let Some(secs) = self.bounded {
            if self.started.elapsed() >= Duration::from_secs(secs) {
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
        }

        // The table's own window, beside the lobby. It opens the moment this
        // client has a seat — founding a table or being given one — and it is
        // where the roster fills up in front of the player.
        if self.state.seated.is_some() && !self.table_closed {
            self.screen = Screen::Table;
        }
        if self.state.seated.is_none() {
            // Left the table, or was refused a seat. The window has nothing to
            // show and closing it is not a decision the player has to make.
            self.screen = Screen::Lobby;
            self.table_closed = false;
        }
        if self.screen == Screen::Table {
            self.table_window(&ctx);
        }

        {
            {
                let view = self.state.view();
                match render::lobby(ui, &view, &mut self.ui) {
                    render::LobbyAction::Select(key) => self.state.selected = Some(key),
                    render::LobbyAction::None => {}
                    render::LobbyAction::OpenTableWindow => {
                        self.screen = Screen::Table;
                        self.table_closed = false;
                    }
                    render::LobbyAction::Create(t) => self.tell(NodeCommand::CreateTable {
                        kind: t.kind,
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
                    render::LobbyAction::Say(text) => self.tell(NodeCommand::SayInLobby(text)),
                    render::LobbyAction::Save(mut settings) => {
                        settings.repair(&self.app_key);
                        ctx.set_zoom_factor(settings.zoom());
                        self.state.me = settings.nickname.clone();
                        self.tell(NodeCommand::SetNickname(settings.nickname.clone()));
                        // Saved to disk, and said either way. A setting that
                        // silently did not persist is one the player changes
                        // again next time and blames the client for.
                        match p2p_poker::storage::settings::save(
                            &self.profile_dir,
                            &settings,
                            &self.app_key,
                        ) {
                            Ok(()) => self.state.log.push_back("settings saved".into()),
                            Err(e) => self
                                .state
                                .log
                                .push_back(format!("the settings did not save: {e}")),
                        }
                        self.ui.settings = settings;
                    }
                }
            }
        }

        // Nothing here. The window is repainted when egui has input for it and
        // when the node has something to say - the relay in `windowed` calls
        // `request_repaint` as each event arrives - and at no other time.
        //
        // The one exception is a run with a deadline, which has to be woken to
        // notice it has passed. Coarse on purpose: a second's imprecision on
        // `--for` costs nothing, and asking every 250 ms would put the busy
        // loop back for the scripted runs that use it.
        if self.bounded.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure the user actually hit, built as the variant `eframe` returns
    /// rather than as the sentence it prints.
    #[test]
    fn the_painter_refusing_opengl_earns_a_second_attempt() {
        let e = eframe::Error::OpenGL(eframe::egui_glow::PainterError::from(
            "egui_glow requires opengl 2.0+. ".to_owned(),
        ));
        assert!(is_a_driver_problem(&e));
        // And the sentence really is the one the user saw, so the two halves of
        // this fix are talking about the same failure.
        assert!(e.to_string().contains("egui_glow requires opengl 2.0+"));
    }

    /// Narrow on purpose: a machine with no display must not be handed to a
    /// renderer that also needs one, and a fault in this program must not be
    /// repeated in a second process.
    #[test]
    fn our_own_faults_do_not_earn_a_second_attempt() {
        let mine = eframe::Error::AppCreation(Box::new(std::io::Error::other("my fault")));
        assert!(!is_a_driver_problem(&mine));
    }

    #[test]
    fn the_child_is_told_which_renderer_and_keeps_every_other_flag() {
        let args: Vec<String> = ["p2p-poker.exe", "--profile", "D", "--host", "T", "--seats", "6"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            software_args(&args),
            vec!["--profile", "D", "--host", "T", "--seats", "6", "--renderer", "software"]
        );
    }

    /// A `--renderer` of the user's own goes, and so does its value. Leaving
    /// `auto` behind would put the child in the same state as the parent, and
    /// every child would start another child.
    #[test]
    fn the_users_own_renderer_choice_is_removed_with_its_value() {
        let args: Vec<String> = ["p2p-poker.exe", "--renderer", "auto", "--profile", "D"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let out = software_args(&args);
        assert_eq!(out, vec!["--profile", "D", "--renderer", "software"]);
        // The value went with the flag: `auto` is not sitting where `--profile`
        // would read its directory.
        assert_eq!(out.iter().filter(|a| *a == "auto").count(), 0);
    }

    /// A trailing `--renderer` with nothing after it must not eat a flag that
    /// is not there, nor panic.
    #[test]
    fn a_trailing_renderer_flag_is_harmless() {
        let args: Vec<String> = ["p2p-poker.exe", "--table", "--renderer"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(software_args(&args), vec!["--table", "--renderer", "software"]);
    }
}

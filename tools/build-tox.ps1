<#
.SYNOPSIS
    Builds what `--features tox` needs from the vendored source in this
    static libsodium. Run once; `cargo build --features tox` does the rest.

.DESCRIPTION
    D-019 moves a table's game traffic off libp2p and onto a Tox NGC group,
    because a public libp2p relay circuit grants 128 KiB and two minutes and a
    Tox session has no per-circuit byte cap. `c-toxcore` is GPL-3.0, so linking
    it makes this whole client GPL-3.0 — which is why it is behind a cargo
    feature rather than an unconditional dependency.

    WHAT THIS SCRIPT DOES AND DOES NOT DO

    It fetches both trees at **pinned commits** and builds libsodium. It does
    not build toxcore: `build.rs` compiles those sixty C files with the `cc`
    crate, reading the file list out of the vendored `CMakeLists.txt` so a
    version bump cannot leave the two out of step.

    WHY THE SOURCES ARE FETCHED RATHER THAN COMMITTED

    `vendor/ziffle` is committed, so vendoring is this project's habit, and the
    same argument — reproducible, offline, reviewable — applies here. What is
    different is that these two are 12.7 MB and the transport they are for is
    not proven yet: the point of D-019 is a measurement across two networks that
    has not been taken. Pinning by commit hash is reproducible without deciding
    the repository's shape for a dependency that may still change version.

    When the transport is measured and the licence change is made formally, the
    right move is to commit them and delete this argument.

.NOTES
    The MSVC path needs no cmake, no pkg-config and no vcpkg. toxcore's own
    CMake build requires all three on Windows; `build.rs` bypasses it, and
    `tools/msvc-shim/pthread.h` supplies the fifteen pthread calls toxcore uses
    so PThreads4W is not a dependency either.
#>

[CmdletBinding()]
param(
    [switch] $Force
)

$ErrorActionPreference = 'Stop'

# Pinned. A moving branch is a build that changes under you, and this one
# changes the licence of the binary it produces.
$TOXCORE_TAG    = 'v0.2.23'
$TOXCORE_COMMIT = 'd9ca3c577e5abd4303d180eb5270167b4133ea4c'
$CMP_COMMIT     = '52bfcfa17d2eb4322da2037ad625f5575129cece'
$SODIUM_TAG     = '1.0.20-RELEASE'
$SODIUM_COMMIT  = '9511c982fb1d046470a8b42aa36556cdb7da15de'

function Step($m) { Write-Host "==>   $m" -ForegroundColor Cyan }
function Note($m) { Write-Host "      $m" -ForegroundColor DarkGray }
function Fail($m) { Write-Host "FAIL  $m" -ForegroundColor Red; exit 1 }

$root   = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $root 'vendor'

# **The vendored trees are committed to this repository, so nothing is fetched.**
#
# They used to be cloned here at a pinned commit and patched on the way past.
# That has a failure mode a committed tree does not: a library that looks right,
# links, runs and is missing the fix. This project has already lost a day to a
# defect that only showed under measurement, and it now carries two patches to
# toxcore rather than none.
#
# So this checks that the source is present. The commit ids above are what the
# trees were taken from and are kept for a reader and for a version bump; there
# is no `.git` here to interrogate any more, and that is the point -- the answer
# is in this repository's own history, and `patches/` is the delta against those
# commits.
function Have($name, $probe) {
    $dir = Join-Path $vendor $name
    if (-not (Test-Path (Join-Path $dir $probe))) {
        Fail "$name is missing from vendor/. It is committed to this repository, so this is a partial checkout or a deleted directory - restore it with 'git checkout -- vendor/$name'."
    }
    Note "$name present"
    return $dir
}

$tox = Have 'c-toxcore' 'toxcore/group_chats.c'

# `third_party/cmp` came from a submodule of the original clone. Its
# `cmp.c` is the first entry of `toxcore_SOURCES`, so without this the build
# fails on a missing file rather than on a missing dependency.
# `third_party/cmp` was a submodule of the clone and is committed here with the
# rest of the tree. Its `cmp.c` is the first entry of `toxcore_SOURCES`, so
# without it the build fails on a missing file rather than a missing dependency.
if (-not (Test-Path (Join-Path $tox 'third_party/cmp/cmp.c'))) {
    Fail "third_party/cmp/cmp.c is missing. It is committed to this repository; restore it with 'git checkout -- vendor/c-toxcore'."
}

# --- our patches ------------------------------------------------------------
#
# **They are already in the tree, because the tree is committed.** `patches/`
# is no longer applied here; it is the record of what this repository's copy of
# c-toxcore differs from upstream by, which is what a reader needs and what a
# version bump needs.
#
# What is checked is that the patched lines are still there. A tree that was
# reverted, half-merged or restored from an unpatched copy would otherwise build
# clean and be missing the fix -- the exact failure that cost this project a day
# and the reason the source is committed at all.
#
# Each entry is a marker unique to one patch and a sentence saying what is lost
# without it. Adding a patch means adding a line here; that is deliberate
# friction, because a patch nothing verifies is a patch that can quietly stop
# existing.
$patched = @(
    @{ File   = 'toxcore/onion_client.c'
       Marker = 'set_tcp_onion_status(nc_get_tcp_c(onion_c->c), true)'
       Why    = '0001: without it a UDP-healthy node keeps no TCP relay, so its invite confirmations carry none and a relayed joiner can never complete a group join' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: ask for the missing message on this path too'
       Why    = '0002: without it a receive ring that has wrapped drops every further packet without ever asking for the message it is missing, and the peer never recovers' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: every confirmed peer, or this is not a send'
       Why    = '0003: without it a custom packet accepted by one peer of ten is reported to the caller as sent, so the application drops it and the other nine never see it' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: the awaited chunk goes in even if its slot is taken'
       Why    = '0004: without it a fragment sequence blocked on an occupied slot can never be unblocked, because unlike an ordinary packet the awaited chunk must be stored - the peer is lost until the sender times it out' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'no room for the first chunk of a fragmented packet'
       Why    = '0005: diagnostic. Without it the fragmenting send path fails silently three times over, and the conservation identity that found S1-AM (array failures = receive drops + send refusals) is left with an unexplained residue' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'is confirmed but cannot be sent to'
       Why    = '0006: corrects 0003. Without it the denominator counts peers send_lossless_group_packet refuses outright - not handshaked, or pending delete - so one peer mid-handshake makes every send fail for ever and one re-handshaking peer stops the whole table' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'Failed to create %s array entry'
       Why    = '0007: diagnostic. create_array_entry is called from both add_to_send_array and store_in_recv_array and logged one sentence for both, so every count of it is the SUM of back-pressure and head-of-line blocking - two faults with nothing in common, and a whole day of readings was taken against the sum' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: this id must not be truncated, because it is written back'
       Why    = '0008: without it a fragmented send that fails part-way rewinds gconn->send_message_id to send_message_id % 65536 - clear_send_queue_id_range assigns start_id back - so past the first 65 536 messages to a peer the whole stream jumps backwards by up to 65 535, every later message reuses a consumed id, and that peer is silently broken for the rest of the session' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: receiving a packet is not requesting one'
       Why    = '0009: without it handle_gc_lossless_helper stamps last_requested_packet_time on every successfully handled packet, and that field is the sole gate on GR_ACK_REQ - so a receiver cannot ask for a missing message for the rest of any second in which it handled anything, the fast one-RTT repair path is switched off, and recovery falls to the senders blind retry whose floor is three seconds' },
    @{ File   = 'toxcore/tox.h'
       Marker = 'p2p-poker: ask the peer for the message this stage is waiting on'
       Why    = '0011: without it the application cannot tell the carrier which message a stalled stage is waiting for, so the one-round-trip GR_ACK_REQ repair is only ever reached by accident - when a LATER message happens to arrive and reveal the hole. A stage waiting on a single seat has nothing later to reveal it, so recovery falls to the blind ladder at T+3/+5/+9/+17/+33 and the 30 s stage budget expires first, certifying out a seat whose message was in flight' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: how many messages are stalled behind a hole'
       Why    = '0011: without it the vote cannot tell a peer the carrier is mid-delivery with from a peer that is genuinely silent, and accuses both. recv_array holds messages that arrived out of order, so a non-zero count is positive evidence the peer is talking' },
    # **The retransmit ladder is frozen by this list.** `CARRIER_LADDER_LAST_MS`
    # in src/protocol/constants.rs is derived from `gcc_resend_packets` firing on
    # `delta > 1 && is_power_of_2(delta)`, and nothing in a Rust build can notice
    # if that changes. 0011 deliberately adds a new entry point beside the ladder
    # rather than editing it, and no later patch may edit it either: change the
    # ladder and the constant silently becomes a lie.
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'delta > 1 && is_power_of_2(delta)'
       Why    = 'the retransmit ladder T+3/+5/+9/+17/+33, which src/protocol/constants.rs mirrors as CARRIER_LADDER_LAST_MS and asserts the stage budget against. If this marker is gone the ladder was edited and that constant is stale - re-derive it before anything else' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: a claim must not overwrite a working direct address'
       Why    = '0012: without it handle_gc_ping stores whatever address a ping carries, unconditionally and silently, over the address we are receiving from at that moment. The claim is the senders DHT self-image, which prefers a non-LAN echo, so for a LAN peer it is the router - and every send to that peer then goes to the router and dies while sendto reports success. Measured: one lossless message re-sent 31 times over 57 s and never accepted; the founder voted out for it; the same shape five and six times a run earlier in the day' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: my WAN address is poison to a peer on my own LAN'
       Why    = '0012: without it the ping sender offers its WAN self address to peers on its own LAN, and never stamps last_sent_ip_time, so the sixty-second interval throttles nothing and every ping to a non-direct peer carries the address' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'arrived from %s:%u but is held at %s:%u'
       Why    = '0013: instrumentation. Without it no line on any node says what address a peer arrived from against the address held for it, and S1-BN - one direction of one pair dark for exactly the 58 s peer timeout while the other flowed and both carriers reported clean - could not be decided from logs' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'changed a peer''s address from %s:%u to %s:%u'
       Why    = '0013: instrumentation. Without it a change to a peer''s stored address by a ping or a handshake leaves no trace - gcc_set_ip_port has no logger - which is why what re-addressed the pair in S1-BN could not be seen' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'are now %s", net_ip_ntoa'
       Why    = '0013: instrumentation. Without it a connection crossing between direct and relayed sends is silent, and the direct branch returns the bare sendto result with no fallback - S1-BN' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'self udp status %u -> %u, self address'
       Why    = '0013: instrumentation. Without it the address this node offers to peers in pings - ipport_self_copy, preferring a non-LAN echo - changes with no trace' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'refused a non-LAN address %s:%u over a LAN one'
       Why    = '0014: without it a ping can replace the LAN address we hold for a peer with a public one, and 0012 does not stop it in exactly the state it happens in - 0012 refuses only while the peer is heard directly within 16 s, and the first instrumented run showed three such overwrites at 96 s with 0012 firing zero times' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'offered self address %s:%u to a peer held at'
       Why    = '0014: instrumentation. Without it the sender side of an address offer is invisible; the receiver logs what it stored and nothing logs what was sent' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0016): one-directional deafness'
       Why    = '0016: the test instrument S1-BO needs. Patch 0015 fires only on an ASYMMETRIC peer timeout, and nothing above this layer can make one: the application''s outage knob leaves the transport alive on purpose, so the pings keep the peer timer fed. A node that ignores its peers'' lossless and lossy group packets for longer than GC_CONFIRMED_PEER_TIMEOUT while still sending to them times them out, they keep it, and its handshake requests then land on connections they still hold. Entirely inside #ifdef P2P_POKER_FAULT_HARNESS, which build.rs defines only under the cargo feature' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: both rings cleared on a re-handshake'
       Why    = '0015: without it an in-place re-handshake resets the message-id counters and the key and leaves both rings full of entries under the old numbering; the new stream collides with them and the peer is timed out again, 2 to 177 s later and then for the rest of the run (S1-BO: 44 of the 63 timeouts in two runs follow a re-handshake on the same node; the other 19 are S1-BN)' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 're-handshaking (%u send, %u recv stale ring entries cleared)'
       Why    = '0015: the call site in handle_gc_handshake_request, and the count in the line the register reads' },
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = 'p2p-poker: no relay carried this packet for connection'
       Why    = '0017: without it the -1 from send_packet_tcp_connection is silent, and five different states produce it -- slots still handshaking, slots killed, slots never registered, the friend limit reached, or no slot at all. S1-AA''s next obstacle cannot be told apart from any run without this line' },
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = 'an out-of-band REGISTERED relay carried this packet'
       Why    = '0018: without it a REGISTERED slot that DOES carry a packet is invisible -- 0017''s counters live inside if (!sent_any) -- so "out-of-band never works" cannot be told from "out-of-band works and is never logged", and out-of-band is the only path a FIRST group handshake can take' },
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = 'a TCP relay is being killed and %u connection(s) lose a slot'
       Why    = '0019: the one-way door, counted where it swings. do_tcp_conns kills a relay that never reached TCP_CONN_CONNECTED instead of reconnecting it, and kill_tcp_relay_connection then zeroes that slot on EVERY con_to -- and nothing in the group code puts one back, because every caller of add_tcp_relay_connection there needs something FROM the peer. It is the only irreversible step in S1-AA''s chain and nothing said when it fires. The count must be taken before the removals, since afterwards it is always zero' },
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = '(%u online, %u registered, %u neither)'
       Why    = '0020: 0019 counts rm_tcp_connection_from_conn returning >= 0, and that matches the relay number at ANY status -- so "7 connections lose a slot" cannot tell seven working out-of-band paths from seven attempts that never registered, and those two readings say opposite things about whether the kill narrowed anybody transport. Same defect 0018 fixed elsewhere: a counter conditioned on something other than the fact it is read for' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: deleting group peer'
       Why    = '0021: a group peer entry disappeared in silence, and that silence is S1-AA''s remaining question. The stalled far seat in split150251-9 holds group 0 seen/0 confirmed at all thirteen samples and emits zero handshake packets, so it has no peer entry to send one to -- and nothing on disk could tell "the entry was created and reaped at the 12 s unconfirmed timeout" from "it was never created", which are different bugs at different layers. do_peer_delete is the one site every deletion passes through and the only one that already holds the logger' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'invite confirmation accepted, handshake armed for peer'
       Why    = '0022: gc_accept_invite creates the inviter entry with NO address and sets no pending_handshake_type; that is set only when the founder confirmation comes back, and this is the one moment a joiner becomes able to handshake at all. It was silent, so a joiner sitting at handshake_attempts 0 until the 12 s unconfirmed reaper takes the entry -- measured four times over a run -- could not be told from one whose confirmation arrived and failed afterwards' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'invite confirmation arrived but the peer entry is gone'
       Why    = '0022: the other half. The -3 exit fires when the confirmation arrives for a peer the reaper has already taken, which is one of the two hypotheses S1-AA is down to, and it was one of four silent exits in that function' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'NOT sending the invite confirmation to friend'
       Why    = '0023: -Stall freezes the joiner event loop and every reading built on it turned out to be about a frozen process -- with the stall firing once the client own recovery works, 17 hands on nine seats. What has never been modelled is what S1-AA is about: a joiner that IS iterating and whose confirmation is slow. Dropping at the inviter rather than the receiver is deliberate, since the friend connection is lossless. Entirely inside ifdef P2P_POKER_FAULT_HARNESS' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'no TCP relay carried it either'
       Why    = '0018: the send is its own oracle. gconn->tcp_relays_count has one write in the tree and no decrement, while the slots it describes are zeroed whenever a relay that never connected is killed, so both gates read the record the send does not use: the warning could never fire once any relay had been saved, and the send was skipped whenever the count was zero even if slots existed' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'udp ret %d, udp skipped %d'
       Why    = '0017: the group-side half. Four handshake failures used to print four identical lines naming only the request type, so they could not be attributed to a peer at all; tcp_relays_count is what GATED the branch and tcp_connection_num is what the send used, and they are different records' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: read the id BEFORE the entry is wiped'
       Why    = '0010: without it process_recv_array_entry acks every drained message with id 0, because clear_array_entry zeroes the struct before array_entry->message_id is read - so the senders slot is never cleared, its time_added never moves, and gcc_resend_packets drops the peer at 58 s unless a later blind duplicate happens to ack it correctly' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: asking peer %u for messages %llu..%llu'
       Why    = '0024: the lossless channel gives a receiver back ONE missed message per request, one request per second, each waiting on some later packet to trigger it - so a 20 s downlink outage drained for 12 s more and cost the seat its place (runs/split162916-9, S1-CO). The receiver now asks every second for everything it is missing, up to sixteen ids at a time, and probes a quiet peer for its head: standard GR_ACK_REQ packets an unpatched sender answers or ignores.' }
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker (patch 0025): the wire is cut for this node'
       Why    = '0025: the uplink half of an outage. Patch 0016 drops what arrives and the seat keeps sending, so every deaf run measured only the downlink; with P2P_POKER_DEAF_UPLINK=1 gcc_send_packet also drops this node own lossless and lossy group packets during the window, after the ring took them, and reports success as a cut wire would. Handshakes pass. The window and its anchor are 0016 own (group_chats.c).' }
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = 'p2p-poker: relay %d goes to sleep'
       Why    = '0026: the relay-slot lifecycle was silent. A relay going to sleep sets every slot on it to NONE, and only ONLINE slots lock a relay, so the out-of-band path of a peer with no direct route -- REGISTERED, never ONLINE -- goes with the relay and nothing said so (runs/split123725-9: the far seat at n3 read 0 online, 0 registered, 4 other from the second a symmetric outage ended). Sleep, wake, the connection-to transitions and both callbacks now say what they do, with the counts that decide the sleep rule.' }
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = 'p2p-poker (patch 0027): a waking connection-to takes its sleepers back.'
       Why    = '0027: set_tcp_connection_to_status(true) never returned the sleepers that sleeping added, so every sleep-wake-sleep cycle of a direct peer left a phantom sleeper on its relays, lock_count == sleep_count held with an awake peer still ONLINE, and do_tcp_conns slept the relay under it -- six of eight local peers lost the far seat in one second when a 20 s outage of one seat ended (S1-CQ, runs/split130548-9). The wake now mirrors the sleep, and a REGISTERED slot of an awake connection-to counts as a use in the sleep rule and the reaper. Local behaviour only.' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: handshake %s attempt %u to peer %u left by %s'
       Why    = '0028: S1-AA join phase. A reaped peer showed attempts 4 with both ends relay-registered within seconds, and neither which attempts left nor why the ones that arrived were dropped was said anywhere -- a routed TCP send prints nothing on success and five receive-side exits were silent. One line per attempt with its door, and one per silent drop with its reason.' }
    @{ File   = 'toxcore/group_chats.h'
       Marker = 'p2p-poker (patch 0029): an unconfirmed peer gets thirty seconds, not twelve.'
       Why    = '0029: S1-AA join phase. Twelve seconds hold four handshake attempts, two of them by TCP, and a peer on another network needs the TCP ones; the far seat reaped three seats at +12 s while their own requests were in flight and then answered them on deleted entries (runs/split140414-9). Thirty seconds hold five TCP attempts; a dead entry lingers eighteen seconds longer, which nothing depends on.' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0030): the same greeting heard again'
       Why    = '0030: S1-DQ. A second table in the same client heard the greeting into its group twice and the second reading reset the first; a repeated request is answered again and a repeated response dropped, nothing reset.' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0031): a peer that left on purpose is forgotten'
       Why    = '0031: S1-DS. A member that quit was kept for reconnection like one that timed out and greeted again for ever; a QUIT is forgotten like a KICK.' }
    @{ File   = 'toxcore/tox.h'
       Marker = 'p2p-poker (patch 0032): how many seconds ago the last packet from this peer'
       Why    = '0032: S1-DT. The library keeps a confirmed member for 58 s after its last packet and the public API did not say how long it had been quiet; the felt reads a member quiet for twenty seconds as off the line.' }
    @{ File   = 'toxcore/tox.h'
       Marker = 'p2p-poker (patch 0033): the friend this member came into the group through'
       Why    = '0033: S1-DV. Before the first hand the group has taught no application key, so an exit or a silence mapped to no seat; the friend number the invitation travelled over is known to the library on both sides and this reads it, so a founder knows every seat it invited from the join.' }
    @{ File   = 'toxcore/tox.h'
       Marker = 'p2p-poker (patch 0034): the table''s word removes a member'
       Why    = '0034: D-045, the owner''s rule. No ghost in a table''s group: a member the players'' word removed is dropped here and, for good, never added again; and a founder''s kick counts only for a key that word allowed, so no founder throws a legitimate seat out. The wire is untouched.' }
    @{ File   = 'toxcore/net.c'
       Marker = 'p2p-poker (patch 0035): the line goes away'
       Why    = '0035a: S1-EB''s instrument. The application''s outage knob (-LinkDownAt) drops table messages above a transport that stays up, so the library never forgets anybody; a real outage makes it forget every member at 58 s and the owner''s deadlock lives past that. Entirely inside #ifdef P2P_POKER_FAULT_HARNESS' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0035): an invitation to a group this client holds with'
       Why    = '0035b: S1-EB. An invitation to a chat this client holds was swallowed in Messenger.c; a member back from an outage holds an emptied copy of the table''s group and its founder''s fresh invitation never reached the application, so a seat whose old address no longer answered could never be brought back. Delivered when the chat holds nobody else; the wire is untouched.' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0036): a peer''s relay connection sleeps only while a'
       Why    = '0036: S1-FC. A member heard directly but held with no address had its relay connection put to sleep, so nothing could be sent to it once no other connection kept that relay awake: a seat back from a restart was unreachable for 55 s and spent a heads-up return. Local; the wire is untouched.' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0037): the library''s own ways back, off, for the bed.'
       Why    = '0037: S1-FE''s instrument. P2P_POKER_NO_LIBRARY_RECONNECT=1 turns off the timed-out list and the saved-peers reseed, so a run measures the invitation road alone; on a relayed line those roads finish their handshake or not by chance. Entirely inside #ifdef P2P_POKER_FAULT_HARNESS' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0038): a sync that names a member removed for good'
       Why    = '0038: S1-FZ. A sync response naming a member the table''s word removed for good hit patch 0034''s refusal, which unpack_gc_sync_announce read as an impossible value, and LOGGER_FATAL aborted the whole client -- a founder died 4 s after giving a silent seat back (run153101-3). The member is skipped; the wire is untouched.' }
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker (patch 0039): a leftover in the receive ring is cleared, never replayed.'
       Why    = '0039: S1-II. The receive ring''s drain took whatever sat in the awaited message''s slot for the awaited message, so an entry left behind a lap of the ring earlier -- a copy stored out of order and then overtaken by its own retransmission -- was replayed, acknowledged under its old id and counted as the awaited message, and the real one was dropped as a duplicate: a lossless stream lost one message to one receiver, and a hand stalled on a seat that lacked what every other seat had. The sender''s half is upstream''s own line, Wrap-around on message N with N from the first lap' }
    @{ File   = 'toxcore/TCP_connection.c'
       Marker = 'p2p-poker (patch 0040): and never in a group''s own instance.'
       Why    = '0040a: S1-AA. A relay sleeps when every connection that locks it is asleep, which is a messenger''s economy; in a group the relay a member is announced by is the only door a relay-only member can knock at, and an out-of-band request is delivered only to a client CONNECTED to the relay. A group''s own TCP instance keeps its relays awake; net_crypto''s is untouched' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0040): a peer the first attempt did not reach is looked for on'
       Why    = '0040b: S1-AA. A member learned from a sync is known by ONE relay, drawn at random by whoever answered (the announce has room for one, and that is the wire''s); a seat that cannot connect to it had no road to that member for the thirty seconds its entry lives, reaped it, and the founder gave the seat away (runs/split201156-9; on demand, runs/split214815-9). A peer that has not shaken hands by the second attempt is registered on up to four of this client''s own connected relays' }
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker (patch 0040, the fault harness): P2P_POKER_DEAD_ANNOUNCED_RELAY=1'
       Why    = '0040c: S1-AA''s instrument. The knob that makes the one announced relay of a relay-only member unreachable, and P2P_POKER_NO_0040, which switches the patch off so a control and its treatment are one binary (tools/table-run-split.ps1 -DeadAnnouncedRelay, -NoPatch0040). Entirely inside #ifdef P2P_POKER_FAULT_HARNESS' }
)

Step 'checking the patches are in the vendored source'
foreach ($p in $patched) {
    $path = Join-Path $tox $p.File
    if (-not (Test-Path $path)) { Fail "$($p.File) is missing from the vendored tree" }
    # **`-CaseSensitive`, and it is not pedantry.** `Select-String` matches
    # case-insensitively by default, so a marker survived being changed from
    # `lose a slot` to `lose a SLOT` -- found by trying to falsify a new marker
    # on 2026-09-07 and failing to. A marker is a literal from a C source file
    # and C string literals are case-sensitive, so the gate must be too:
    # otherwise it certifies as present a line the binary does not contain.
    if (-not (Select-String -Path $path -SimpleMatch -CaseSensitive -Pattern $p.Marker -Quiet)) {
        Fail "$($p.File) does not contain the patch marker '$($p.Marker)'. $($p.Why). Restore the tree with 'git checkout -- vendor/c-toxcore', or re-apply patches/ if you are moving to a new upstream."
    }
    Note "  $($p.File): patched"
}

$sodium = Have 'libsodium' 'src/libsodium/sodium/core.c'

# --- libsodium, static, x64 -------------------------------------------------
$lib = Join-Path $sodium 'bin\x64\Release\v143\static\libsodium.lib'
if ((Test-Path $lib) -and -not $Force) {
    Note "libsodium.lib already built"
} else {
    Step 'building libsodium (static, x64) with MSBuild'
    # The Build Tools' own MSBuild. Not `Get-Command msbuild`, which finds
    # whatever a stray PATH entry points at.
    $msb = Get-ChildItem `
        'C:\Program Files*\Microsoft Visual Studio\2022\*\MSBuild\Current\Bin\MSBuild.exe' `
        -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
    if (-not $msb) {
        Fail 'no MSBuild from Visual Studio 2022. Install the C++ Build Tools; the Rust toolchain already needs its linker.'
    }
    $sln = Join-Path $sodium 'builds\msvc\vs2022\libsodium.sln'
    & $msb $sln /p:Configuration=StaticRelease /p:Platform=x64 /v:minimal /m
    if ($LASTEXITCODE -ne 0) { Fail 'libsodium did not build' }
}
if (-not (Test-Path $lib)) { Fail "libsodium built but $lib is not there" }
Note "libsodium: $lib"

Write-Host ""
Write-Host "RESULT  ready. Build with:" -ForegroundColor Green
Write-Host "        cargo build --features tox"
Write-Host ""
Write-Host "        Note that --features tox links GPL-3.0 code and makes the" -ForegroundColor Yellow
Write-Host "        resulting binary GPL-3.0. That is D-019's decision, not this" -ForegroundColor Yellow
Write-Host "        script's, and it is a one-way door." -ForegroundColor Yellow

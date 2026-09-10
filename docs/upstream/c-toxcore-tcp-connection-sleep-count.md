# Draft for upstream: `set_tcp_connection_to_status(..., true)` never returns the sleepers it added

**Status: draft, not filed.** Written 2026-09-10 from `S1-CQ` in `docs/DECISIONS.md`.
Filing it is the owner's call; nothing here has been sent anywhere.

Target: `c-toxcore` v0.2.23, `toxcore/TCP_connection.c`. Our vendored copy carries
the fix as `patches/0027-a-waking-connection-to-takes-its-sleepers-back.patch`
(part A); part B of that patch is our own policy and not part of this report.

## Summary

`TCP_con.sleep_count` is meant to count the `ONLINE` slots on a relay whose
`TCP_Connection_to` is asleep. `set_tcp_connection_to_status(tcp_c, n, false)`
(a connection-to going to sleep) increments it once per `ONLINE` slot;
`set_tcp_connection_to_status(tcp_c, n, true)` (the connection-to waking) does
not decrement it -- the wake branch only marks sleeping relays `unsleep`. Every
sleep-wake-sleep cycle of a connection-to therefore leaves one phantom sleeper
on each relay it is `ONLINE` on. Once `lock_count == sleep_count` holds while an
*awake* connection-to is still `ONLINE` on the relay, `do_tcp_conns` puts the
relay to sleep under it (`sleep_tcp_relay_connection`), which kills the TCP
client and sets every slot on that relay to `TCP_CONNECTIONS_STATUS_NONE` --
the awake peer's included. A peer that had no other route (no direct UDP,
reached only through that relay) is then unreachable until something else
wakes the relay, and nothing does on its behalf: `set_tcp_connection_to_status(true)`
on an already-awake connection-to is a no-op.

## How we hit it

Nine group-chat peers, eight on one LAN (direct UDP between them) and one on
another network reachable only through TCP relays. A 20-second outage of one
LAN peer makes each of its neighbours' connection-to for it wake at +16 s
(`do_gc_tcp` calls `set_tcp_connection_to_status` with `!gcc_conn_is_direct`
every tick) and sleep again when the outage ends -- exactly one cycle per
neighbour. With eight peers `ONLINE` on a relay and seven of them asleep, the
one phantom makes `lock_count 8 == sleep_count 8`, and the relay sleeps with
the awake relay-only peer's `ONLINE` slot on it. In our logs, six of eight LAN
peers lost the relay-only peer in the same second, each then failing every
send to it ("no relay carried this packet", ~2 000 per peer per minute) until
the 58-second peer timeout deleted it at both ends.

Instrumented lines from one such moment (our debug patch, not upstream code):

```
284.7  connection-to 7 sleeps (peer direct)
284.7  relay 0 goes to sleep (lock_count 8 == sleep_count 8): 8 online slot(s), 0 registered, ...
284.7  relay 1 goes to sleep (lock_count 8 == sleep_count 8): 8 online slot(s), 0 registered, ...
284.7  relay 2 goes to sleep (lock_count 8 == sleep_count 8): 8 online slot(s), 0 registered, ...
286.2  no relay carried this packet for connection 0: 0 online, 0 registered, 3 other, 6 slots
```

Connection-to 0 is the relay-only peer; it had been `ONLINE` on relay 0 with
`lock_count 1, sleep_count 0` (awake) since 258.0 s and never slept.

## Fix

In the wake branch of `set_tcp_connection_to_status`, mirror the sleep branch:

```c
    /* Connection is unsleeping. */
    ...
    for (uint32_t i = 0; i < MAX_FRIEND_TCP_CONNECTIONS; ++i) {
        if (con_to->connections[i].tcp_connection > 0) {
            ...
            if (tcp_con->status == TCP_CONN_SLEEPING) {
                tcp_con->unsleep = true;
            }

            if (con_to->connections[i].status == TCP_CONNECTIONS_STATUS_ONLINE && tcp_con->sleep_count > 0) {
                --tcp_con->sleep_count;
            }
        }
    }
```

With it, a relay an awake peer is `ONLINE` on reads `lock_count > sleep_count`
and stays awake. The same nine-peer outage after the fix: no failed sends, no
timeouts, no peer lost (`runs/split132427-9` in our repository).

## Why a healthy network never shows it

The accounting only drifts when a connection-to cycles sleep-wake-sleep, and a
stable set of direct peers never cycles. It needs a transient loss of the
direct route to one peer -- a brief outage, a NAT mapping expiring -- and a
relay-only peer sharing the relay. Both are ordinary on the public network.

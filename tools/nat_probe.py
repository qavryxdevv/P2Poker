import socket, struct, os, sys

MAGIC = 0x2112A442

def binding_request():
    tid = os.urandom(12)
    return struct.pack(">HHI", 0x0001, 0, MAGIC) + tid, tid

def parse(data, tid):
    if len(data) < 20: return None
    mtype, mlen, magic = struct.unpack(">HHI", data[:8])
    if magic != MAGIC or data[8:20] != tid: return None
    body, i = data[20:20+mlen], 0
    while i + 4 <= len(body):
        atype, alen = struct.unpack(">HH", body[i:i+4])
        val = body[i+4:i+4+alen]
        if atype == 0x0020 and len(val) >= 8:      # XOR-MAPPED-ADDRESS
            port = struct.unpack(">H", val[2:4])[0] ^ (MAGIC >> 16)
            ip = bytes(b ^ c for b, c in zip(val[4:8], struct.pack(">I", MAGIC)))
            return socket.inet_ntoa(ip), port
        i += 4 + alen + ((4 - alen % 4) % 4)
    return None

SERVERS = [("stun.l.google.com", 19302), ("stun1.l.google.com", 19302),
           ("stun.cloudflare.com", 3478), ("stun.nextcloud.com", 3478)]

# ONE local socket, several servers: if the external port is identical for all,
# the NAT mapping is endpoint-independent and hole punching can work.
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("0.0.0.0", 0))
s.settimeout(4)
print(f"local socket: {s.getsockname()}")
results = []
for host, port in SERVERS:
    try:
        addr = (socket.gethostbyname(host), port)
        req, tid = binding_request()
        s.sendto(req, addr)
        data, _ = s.recvfrom(2048)
        r = parse(data, tid)
        print(f"  {host:<24} -> {r}")
        if r: results.append(r)
    except Exception as e:
        print(f"  {host:<24} -> FAILED {type(e).__name__}: {e}")

if len(results) >= 2:
    ips = {r[0] for r in results}
    ports = {r[1] for r in results}
    print()
    print(f"external IPs seen : {ips}")
    print(f"external ports    : {ports}")
    print()
    if len(ports) == 1 and len(ips) == 1:
        print("VERDICT: endpoint-independent mapping (cone NAT).")
        print("         Same external port to every destination -> hole punching is viable.")
    elif len(ports) > 1:
        print("VERDICT: SYMMETRIC NAT - a different external port per destination.")
        print("         Hole punching by address prediction does NOT work. Relay required.")
    else:
        print("VERDICT: inconsistent external IP - multiple WAN paths or CGNAT pool.")
    local_ip = s.getsockname()[0]
    print(f"\nlocal bind {local_ip}; if the external IP differs from every local")
    print("interface address, the host is behind NAT (expected).")
else:
    print("\nVERDICT: could not reach enough STUN servers to classify.")
s.close()

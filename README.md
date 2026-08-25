# rproxy

A blazing fast, cross-platform, transparent **TCP & UDP proxy** written in Rust. No bloat, no config headaches — just simple port forwarding that works out of the box.

Ships with **rproxy-edit**, a built-in terminal UI for managing proxy configurations visually, and **hot reload** support — update your config and apply changes without dropping a single connection.

## Installation

```bash
cargo install rproxy
```

This installs both `rproxy` (the proxy) and `rproxy-edit` (the config editor).

## Quick Start

### Single Proxy

Forward local port 8080 to a remote service:

```bash
rproxy -b 0.0.0.0:8080 -r 10.0.0.5:3000 -p TCP
```

Forward UDP traffic (e.g. DNS, VPN tunnels):

```bash
rproxy -b 0.0.0.0:1194 -r vpn-server.example.com:1194 -p UDP
```

### Multiple Proxies with a Config File

Run many proxies at once from a single JSON config:

```bash
rproxy -c proxies.json
```

Where `proxies.json` looks like:

```json
{
    "settings": {
        "max_connections": 2048,
        "max_client_tunnels": 1024,
        "keepalive_idle": 60,
        "keepalive_interval": 30
    },
    "proxies": [
        {
            "bind": "0.0.0.0:8080",
            "remote": "10.0.0.5:3000",
            "protocol": "TCP"
        },
        {
            "bind": "0.0.0.0:1194",
            "remote": "vpn-server.example.com:1194",
            "protocol": "UDP"
        }
    ]
}
```

The `settings` block is optional — omit it to use defaults. Plain JSON arrays (the old format) are still supported for backward compatibility.

### Edit Configs with the TUI

Tired of editing raw JSON? Open any config in the built-in terminal editor:

```bash
rproxy-edit proxies.json
```

```
┌─────────────────────────────────────────────────┐
│  rproxy config editor — proxies.json            │
├────┬──────────────────┬──────────────────┬───────┤
│  # │ Bind             │ Remote           │ Proto │
├────┼──────────────────┼──────────────────┼───────┤
│> 1 │ 0.0.0.0:8080     │ 10.0.0.5:3000    │  TCP  │
│  2 │ 0.0.0.0:1194     │ vpn-server.ex... │  UDP  │
├────┴──────────────────┴──────────────────┴───────┤
│ [NORMAL] j/k:nav  i:edit  a:add  d:del  s:settings│
└─────────────────────────────────────────────────┘
```

Keybindings (Helix/Vim-style):

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate up/down |
| `i` / `Enter` | Edit selected entry |
| `a` | Add new proxy entry |
| `d` | Delete selected entry |
| `s` | Edit settings (connections, keepalive, etc.) |
| `Tab` / `Shift+Tab` | Cycle fields in edit mode |
| `Space` | Toggle protocol (TCP/UDP) |
| `:w` | Save |
| `:q` | Quit |
| `:wq` | Save and quit |
| `?` | Show help |

## CLI Reference

```
rproxy — A platform neutral asynchronous UDP/TCP proxy

Options:
  -r, --remote <host>:<port>    Remote endpoint to forward traffic to
  -b, --bind <ip>:<port>        Local address to listen on
  -p, --protocol TCP|UDP        Protocol (UDP by default)
  -c, --config <file.json>      JSON config for multiple proxies
  -s, --signal <reload>         Send signal to running rproxy (reload config)
  -d, --debug                   Enable debug logging
  -l, --logger_settings <file>  Logger config file (YAML)

  --max-connections N           Max concurrent TCP connections (default: 1024)
  --max-client-tunnels N        Max concurrent UDP client tunnels (default: 1024)
  --keepalive-idle N            TCP keepalive idle time in seconds (default: 60)
  --keepalive-interval N        TCP keepalive probe interval in seconds (default: 30)
```

CLI flags override values from the config file's `settings` block.

## Use Cases

### Expose a Local Development Service

Forward traffic from a public-facing port to a service running on localhost:

```bash
rproxy -b 0.0.0.0:443 -r 127.0.0.1:3000 -p TCP
```

### OpenVPN / WireGuard Relay

Relay VPN tunnel traffic through an intermediary server. rproxy handles UDP natively with per-client session tracking:

```bash
rproxy -b 0.0.0.0:1194 -r vpn-backend.internal:1194 -p UDP
```

### Multi-Service Gateway

Run a single rproxy instance as a lightweight gateway that fans out to multiple backend services:

```json
[
    { "bind": "0.0.0.0:80",   "remote": "web-server:8080",      "protocol": "TCP" },
    { "bind": "0.0.0.0:443",  "remote": "web-server:8443",      "protocol": "TCP" },
    { "bind": "0.0.0.0:5432", "remote": "db.internal:5432",     "protocol": "TCP" },
    { "bind": "0.0.0.0:53",   "remote": "dns.internal:53",      "protocol": "UDP" }
]
```

```bash
rproxy -c gateway.json
```

### SSH Jump Host Alternative

Forward SSH access to machines behind a firewall without configuring SSH ProxyJump:

```bash
rproxy -b 0.0.0.0:2222 -r 192.168.1.100:22 -p TCP
```

Then connect with: `ssh -p 2222 user@proxy-host`

### Game Server Relay

Relay UDP game traffic to reduce latency or provide a stable entry point:

```bash
rproxy -b 0.0.0.0:27015 -r game-server.region.example.com:27015 -p UDP
```

### Managing Configs Across Environments

Use `rproxy-edit` to maintain separate configs for different environments, then hot-reload without downtime:

```bash
rproxy -c configs/production.json     # start production proxies
rproxy-edit configs/production.json   # edit config in TUI (press 's' for settings)
rproxy -s reload                      # apply changes — zero downtime
```

The editor validates addresses and protocols before saving, so you catch mistakes before they hit production.

## Hot Reload

rproxy supports nginx-style hot reload — update your config file and apply changes without restarting the process or dropping existing connections.

### How It Works

1. Start rproxy with a config file:

```bash
rproxy -c proxies.json
```

2. Edit the config (add, remove, or change proxies) using `rproxy-edit` or any text editor:

```bash
rproxy-edit proxies.json
```

3. Apply changes without restarting:

```bash
rproxy -s reload
```

rproxy compares the old and new config and makes the minimum necessary changes:

| Change | What happens |
|--------|-------------|
| **Proxy unchanged** (same bind, remote, protocol) | No interruption — zero downtime |
| **New proxy added** | Started immediately |
| **Proxy removed** | Stops accepting new connections, existing connections drain gracefully |
| **Proxy changed** (same bind, different remote) | Old proxy drains, new proxy starts |

Existing TCP connections through unchanged proxies are **never dropped**. UDP tunnels through removed proxies receive a clean terminate signal and drain naturally.

### Under the Hood

When started with `-c`, rproxy writes its PID to a private per-user runtime directory (`$XDG_RUNTIME_DIR`, or a `rproxy-<uid>` directory created with mode `0700` under the system temp dir). Running `rproxy -s reload` reads that PID file and sends `SIGHUP` to the process, which triggers the config diff and reload cycle. The PID file is created owner-only (`0600`), its ownership and permissions are verified before it is trusted, and on Linux the target is confirmed to be an rproxy process before signalling — so another local user cannot induce a reload into signalling an arbitrary process.

## Configuration Format

Config files use a JSON object with an optional `settings` block and a `proxies` array. Plain JSON arrays (without the wrapper object) are also supported for backward compatibility.

### Proxy Fields

| Field | Type | Description |
|-------|------|-------------|
| `bind` | `string` | Local `ip:port` to listen on |
| `remote` | `string` | Remote `host:port` to forward to (supports hostnames with auto DNS refresh) |
| `protocol` | `string` | `"TCP"` or `"UDP"` |

### Settings Fields

All settings are optional and have sensible defaults:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_connections` | `integer` | `1024` | Max concurrent TCP connections |
| `max_client_tunnels` | `integer` | `1024` | Max concurrent UDP client tunnels |
| `keepalive_idle` | `integer` | `60` | Seconds of idle before first TCP keepalive probe |
| `keepalive_interval` | `integer` | `30` | Seconds between TCP keepalive probes |

rproxy automatically re-resolves DNS hostnames every 30 seconds, so you can point at dynamic endpoints without restarts.

## Dynamic DNS Resolution

Most proxies resolve a hostname to an IP once at startup and hold onto it forever. If the IP behind that hostname changes, traffic breaks and you have to restart the proxy. rproxy takes a different approach: it **re-resolves DNS every 30 seconds** and seamlessly switches to the new IP with zero downtime.

This matters more than you might think:

### Cloud Instances with Elastic IPs

Cloud providers frequently reassign public IPs when instances are stopped, restarted, or auto-scaled. Your VPN relay points at `vpn.example.com`, and after a routine maintenance window the underlying IP changes from `52.14.88.10` to `52.14.91.33`. A traditional proxy keeps sending packets to the old IP — connections time out and users call you at 3 AM. rproxy picks up the new IP on the next 30-second cycle automatically.

```json
{ "bind": "0.0.0.0:1194", "remote": "vpn.example.com:1194", "protocol": "UDP" }
```

### Kubernetes Services and Rolling Deployments

In Kubernetes, a `Service` or `Ingress` hostname can resolve to different pod IPs after a rolling deployment. If you're proxying traffic into the cluster through an external rproxy, the backend IP may shift every time a deployment rolls out. Dynamic resolution means deployments never cause proxy downtime.

```json
{ "bind": "0.0.0.0:8080", "remote": "my-app.k8s.internal:8080", "protocol": "TCP" }
```

### DNS-Based Failover and Load Balancing

Services like Route 53, Cloudflare DNS, or Consul use DNS records for health-based failover — when a primary server goes down, the DNS record updates to point at a standby. rproxy follows the DNS change within 30 seconds, so failover works end-to-end without manual intervention.

```json
{ "bind": "0.0.0.0:5432", "remote": "db-primary.service.consul:5432", "protocol": "TCP" }
```

### Dynamic Home / Edge Networks

If you're running rproxy on a home server or edge device, the remote endpoint might be behind a dynamic DNS provider (e.g. `myhouse.duckdns.org`). The ISP rotates your IP periodically. rproxy handles this transparently — no cron jobs, no restart scripts.

```bash
rproxy -b 0.0.0.0:2222 -r myhouse.duckdns.org:22 -p TCP
```

### What Happens Under the Hood

Every 30 seconds, rproxy re-resolves the hostname for each proxy. When a new IP is detected, it logs the change and routes all new connections to the updated address — existing connections continue on the old IP until they naturally close. No connections are dropped, no traffic is interrupted.

## License

MIT

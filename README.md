# rproxy 

rproxy is a blazing fast cross-platform transparent TCP&UDP proxy. No bullshit, extremely simple and just out of the box.

## Installation

```
cargo install rproxy
```

## Usage

```
rproxy: A platform neutral asynchronous UDP/TCP proxy

Options:
    -r, --remote <host>:<port>
                        The remote endpoint. e.g. www.xxx.yyy:443
    -b, --bind <ip>:<port>
                        The address to be listened. 0.0.0.0:33333 by default
    -p, --protocol TCP|UDP
                        Protocol of the communication, UDP by default
    -d, --debug         Enable debug mode
```

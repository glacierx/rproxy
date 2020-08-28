#![recursion_limit="256"]
#![warn(rust_2018_idioms)]

// use futures_util::stream::stream::StreamExt;
// use std::future::Future;
use std::net::ToSocketAddrs;
// use tokio::task::JoinHandle;
// use tokio::net::udp::SendHalf;
use futures::io::ErrorKind::AlreadyExists;
use futures::future::try_join;
use getopts::Options;
// use std::pin::Pin;
// use std::sync::Arc;
use std::collections::HashMap;
use std::{env, io};
// use std::net::UdpSocket;
use std::net::SocketAddr;

#[cfg(tcp)]
#[cfg(udp)]

use tokio::net::TcpListener;
use tokio::net::UdpSocket;
use futures::{
    // channel::{
    //     mpsc
    // },
    stream::{
        StreamExt
    },
    // future::FutureExt, // for `.fuse()`
    // future::Future,
    // pin_mut,
    // select,
};

// use tokio::net::

fn usage(program: &str, opts: &Options){
    // let program_path = std::path::PathBuf::from(program);
    // let program_name = program_path.file_stem().unwrap().to_str().unwrap();
    // println!("Usage: {} [-b BIND_ADDR] -l LOCAL_PORT -h REMOTE_ADDR -r REMOTE_PORT", program_name);
    println!("{}: {}", program, opts.usage("A platform independent asynchronous UDP/TCP proxy"));
}
type Tx=futures::channel::mpsc::UnboundedSender<(SocketAddr, Vec<u8>)>;
type Rx=futures::channel::mpsc::UnboundedReceiver<(SocketAddr, Vec<u8>)>;

struct Client{
    peer: SocketAddr,
    remote: SocketAddr,
    send: Tx,
    recv: Rx
}

impl Client {

    async fn run(mut self) -> Result<(), io::Error>{
        let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let (mut socket_recv, mut socket_send) = socket.split();
        // let mut recv = self.recv.clone();
        let client_peer = self.peer;
        let _tx = self.send.clone();
        // let remote_addr: SocketAddr = SocketAddr::from(self.remote);
        let remote_addr = self.remote;
        let client_to_remote_proc = async move {
            // let mut buf: Vec<u8> = vec![0;1024*10];
            loop{
                if let Some((_peer, buf)) = self.recv.next().await {
                    match socket_send.send_to(&buf[..], &remote_addr).await {
                        Ok(_sz) => {

                        },
                        Err(_e) => {
                            return Err(io::Error::from(AlreadyExists));
                        }
                    }
                } else {
                    break;
                }
            }
            Ok(())
        };
        let remote_to_client_proc = async move {
            let mut buf: Vec<u8> = vec![0;1024*10];
            loop{
                if let Ok((_size, _peer)) = socket_recv.recv_from(&mut buf).await{
                    match _tx.unbounded_send((client_peer, buf.clone())) {
                        Ok(_sz) => {

                        },
                        Err(_e) => {
                            return Err(io::Error::from(AlreadyExists));
                        }
                    }
                } else {
                    break;
                }
            }
            Ok(())
        };
        try_join(client_to_remote_proc, remote_to_client_proc).await.unwrap();
        Ok(())        
    }

}
struct Server {
    addr: String,
    remote: String,
}


impl Server {

    async fn run(self) -> Result<(), io::Error> {
        let socket = UdpSocket::bind(&self.addr).await.unwrap();
        println!("Listening on {}", socket.local_addr().unwrap());
        let server: Vec<_> = self.remote
                            .to_socket_addrs()
                            .expect("Unable to resolve domain")
                            .collect();

        let _remote = server[0];
        let (mut socket_recv, mut socket_send) = socket.split();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<(SocketAddr, Vec<u8>)>();
        let remote_to_client_proc = async move {
            loop{
                if let Some((peer, buf)) = rx.next().await {
                    match socket_send.send_to(&buf[..], &peer).await {
                        Ok(_sz) => {

                        },
                        Err(e) => {
                            return Err(e);
                        }
                    }
                } else {
                    break;
                }
            }
            Ok(())

        };
        // let mut client_run_procs: Vec<JoinHandle<Result<(), io::Error>> > = Vec::new();

        let client_to_proxy_proc = async move {
            let mut buf: Vec<u8> = vec![0;1024*256];
            let mut client_tunnels:HashMap<SocketAddr, Tx> = HashMap::new();
            loop{

                let data = Some(socket_recv.recv_from(&mut buf).await.unwrap()); 
                if let Some((size, peer)) = data {
                    // let _addr = format!("{}:{}", peer.ip(), peer.port());
                    match client_tunnels.get(&peer) {
                        Some(_tx) => {
                            _tx.unbounded_send((peer, buf.clone())).unwrap();
                        },
                        _ => {
                            let (mut _s,_r) = futures::channel::mpsc::unbounded::<(SocketAddr, Vec<u8>)>();
                            _s.unbounded_send((peer, buf.clone())).unwrap();
                            client_tunnels.insert(peer, _s);
                            let c = Client {
                                peer : peer,
                                remote: _remote,
                                send: tx.clone(),
                                recv: _r
                            };
                            tokio::spawn(c.run());
                        }
                    }
                    let tx = client_tunnels.get(&peer).unwrap();
                    tx.unbounded_send((peer, Vec::from(&buf[0..size]))).unwrap();
                } else {
                    break;
                }
            }
            Ok(())
        };
        // client_to_proxy_proc.await;
        try_join(client_to_proxy_proc, remote_to_client_proc).await.unwrap();
        Ok(())
    }
}

async fn work(bind_address:String, 
    local_port:u16, 
    remote_address:String, 
    remote_port:u16) -> Result<(), io::Error>
{
    let addr = format!("{}:{}", bind_address, local_port);
    // opts.
    let remote = format!("{}:{}", remote_address, remote_port);
    let server = Server {
        addr,
        remote
    };
    return server.run().await;
}

#[tokio::main]
async fn main(){
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.reqopt("l",
                "local-port",
                "The local port",
                "LOCAL_PORT");
    opts.reqopt("r",
                "remote-port",
                "The remote port",
                "REMOTE_PORT");
    opts.reqopt("h",
                "host",
                "The remote address",
                "REMOTE_ADDR");
    opts.optopt("b",
                "bind",
                "The address to be listened",
                "BIND_ADDR");
    opts.optflag("d", "debug", "Enable debug mode");

    let matches = opts.parse(&args[1..])
        .unwrap_or_else(|_| {
            usage(&program, &opts);
            std::process::exit(-1);
        });
    let bind_addr = matches.opt_str("b").unwrap();
    let local_port = matches.opt_str("l").unwrap().parse::<u16>().unwrap();
    let remote_addr = matches.opt_str("h").unwrap();
    let remote_port = matches.opt_str("r").unwrap().parse::<u16>().unwrap();
    work(bind_addr, local_port, remote_addr, remote_port).await.unwrap();
}
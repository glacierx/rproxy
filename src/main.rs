#![recursion_limit="512"]
#![warn(rust_2018_idioms)]

use std::time::SystemTime;
use std::io::ErrorKind::Other;
use std::net::ToSocketAddrs;
use futures::future::try_join;
use getopts::Options;
use std::collections::HashMap;
use std::{env, io};
use std::net::SocketAddr;
use log::*;
use chrono::{Local};

#[cfg(tcp)]
#[cfg(udp)]

use tokio::net::TcpListener;
use tokio::net::UdpSocket;
use tokio::time;
use futures::{
    channel::{
        mpsc::{
            UnboundedReceiver,
            UnboundedSender,
            unbounded
        }
    },
    stream::{
        StreamExt
    },
    future::FutureExt, // for `.fuse()`
    // future::Future,
    // pin_mut,
    select,
};


fn usage(_program: &str, opts: &Options){
    println!("rproxy: {}", opts.usage("A platform neutral asynchronous UDP/TCP proxy"));
}

#[allow(dead_code)]
enum MessageType{
    Data,
    Terminate,
}

type Tx=UnboundedSender<(SocketAddr, Vec<u8>, MessageType)>;
type Rx=UnboundedReceiver<(SocketAddr, Vec<u8>, MessageType)>;

struct UDPPeerPair {
    client: SocketAddr,
    remote: SocketAddr,
    send: Tx,
    recv: Rx
}

impl UDPPeerPair {

    async fn run(mut self) -> Result<(), io::Error>{
        let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let (mut socket_recv, mut socket_send) = socket.split();
        let client_peer = self.client;
        let _tx = self.send.clone();
        let remote_addr = self.remote;
        let (ctrl_tx, mut ctrl_rx) = unbounded::<MessageType>();

        let client_to_remote_proc = async move {
            // let mut buf: Vec<u8> = vec![0;1024*10];
            loop{

                if let Some((_peer, buf, msg_type)) = self.recv.next().await {

                    match msg_type {
                        MessageType::Terminate => {
                            debug!("{}:{} sends TERMINATE signal", client_peer.ip(), client_peer.port());
                            ctrl_tx.unbounded_send(MessageType::Terminate).unwrap();
                            break;
                        },
                        _ => {}
                    }
                    debug!("Forward {} bytes from {}", buf.len(), _peer);

                    match socket_send.send_to(&buf[..], &remote_addr).await {
                        Ok(_sz) => {

                        },
                        Err(_e) => {
                            panic!(_e);
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
                select!{
                    x = socket_recv.recv_from(&mut buf).fuse() => {
                        if let Ok((_size, _peer)) = x {
                            debug!("Recv {} bytes to {}", _size, client_peer);
                            match _tx.unbounded_send((client_peer, Vec::from(&buf[.._size]), MessageType::Data)) {
                                Ok(_sz) => {
        
                                },
                                Err(_e) => {
                                    return Err(io::Error::from(Other));
                                }
                            }
                        }
                    },
                    y = ctrl_rx.next() => {
                        if let Some(msg_type) = y{
                            match msg_type{
                                MessageType::Terminate => {
                                    debug!("{}:{} recvs TERMINATE signal", client_peer.ip(), client_peer.port());
                                    break;
                                },
                                _ =>{

                                }
                            }
                        }
                    }
                }                
            }
            Ok(())
        };
        try_join(client_to_remote_proc, remote_to_client_proc).await.unwrap();
        debug!("{}:{} exits", client_peer.ip(), client_peer.port());
        Ok(())        
    }

}
struct UDPProxy<'a> {
    addr: &'a String,
    remote: &'a String,
}


impl<'a> UDPProxy<'a> {

    async fn run(self) -> Result<(), io::Error> {
        let socket = UdpSocket::bind(&self.addr).await.unwrap();
        info!("Listening on {}", socket.local_addr().unwrap());
        let server: Vec<_> = self.remote
                            .to_socket_addrs()
                            .expect("Unable to resolve domain")
                            .collect();

        let _remote = server[0];
        let (mut socket_recv, mut socket_send) = socket.split();
        let (tx, mut rx) = unbounded::<(SocketAddr, Vec<u8>,  MessageType)>();
        let remote_to_client_proc = async move {
            loop{
                if let Some((peer, buf, _msg_type)) = rx.next().await {
                    debug!("Forward {} bytes to {}", buf.len(), peer);
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
            let empty: Vec<u8> = vec![0;0];
            let mut client_tunnels:HashMap<SocketAddr, (Tx, SystemTime)> = HashMap::new();
            let mut time_out1 = time::interval(tokio::time::Duration::from_secs(5)).fuse();
            loop{

                select! {
                    data = socket_recv.recv_from(&mut buf).fuse() => {
                        if let Ok((size, peer)) = data {
                            // let _addr = format!("{}:{}", peer.ip(), peer.port());
                            match client_tunnels.get(&peer) {
                                Some((_tx, _active_time)) => {
                                    // _tx.unbounded_send((peer, Vec::from(&buf[..size]), MessageType::Data)).unwrap();
                                },
                                _ => {
                                    info!("New client {}:{} is added", peer.ip(), peer.port());
                                    let (mut _s,_r) = unbounded::<(SocketAddr, Vec<u8>,  MessageType)>();
                                    // _s.unbounded_send((peer, buf.clone(), MessageType::Data)).unwrap();
                                    client_tunnels.insert(peer, (_s, SystemTime::now()));
                                    let c = UDPPeerPair {
                                        client : peer,
                                        remote: _remote,
                                        send: tx.clone(),
                                        recv: _r
                                    };
                                    tokio::spawn(c.run());
                                }
                            }
                            let (tx, tm) = &mut client_tunnels.get_mut(&peer).unwrap();
                            debug!("Recv {} bytes from {}", size, peer);
                            tx.unbounded_send((peer, Vec::from(&buf[..size]), MessageType::Data)).unwrap();
                            *tm = SystemTime::now();
                        } else {
                            break;
                        }
                    },
                    _ = time_out1.next() =>{
                        debug!("Tick");
                        let mut tbd: Vec<SocketAddr> = Vec::new();
                        for (k, v) in (&mut client_tunnels).iter(){
                            let sec = v.1.elapsed().unwrap().as_secs();
                            if sec > 10{
                                info!("Client {}:{} is timeout({}s)", k.ip(), k.port(), sec);
                                v.0.unbounded_send((k.clone(), empty.clone(), MessageType::Terminate)).unwrap();
                                tbd.push(k.to_owned());
                            }
                        }

                        for k in tbd{
                            client_tunnels.remove(&k);
                        }

                    }
                }
            }
            Ok(())
        };
        // client_to_proxy_proc.await;
        try_join(client_to_proxy_proc, remote_to_client_proc).await.unwrap();
        Ok(())
    }
}

async fn udp_proxy(local: &String, 
    remote:&String) -> Result<(), io::Error>
{
    let server = UDPProxy {
        addr: &local,
        remote: &remote
    };
    return server.run().await;
}
static MY_LOGGER: MyLogger = MyLogger;

struct MyLogger;

impl log::Log for MyLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            println!("[{}][{}] - {}", record.level(), Local::now(), record.args());
        }
    }
    fn flush(&self) {}
}



#[tokio::main]
async fn main(){
    log::set_logger(&MY_LOGGER).unwrap();
    log::set_max_level(LevelFilter::Info);
    // info!("Hello world");
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.reqopt("r",
                "remote",
                "The remote endpoint. e.g. www.xxx.yyy:443",
                "<host>:<port>");
    opts.optopt("b",
                "bind",
                "The address to be listened. 0.0.0.0:33333 by default",
                "<ip>:<port>");
    opts.optopt("p",
                "protocol",
                "Protocol of the communication, UDP by default",
                "TCP|UDP");
    opts.optflag("d", "debug", "Enable debug mode");

    if let Ok(matches) = opts.parse(&args[1..]) {
        let local_addr:String = matches.opt_str("b").unwrap();
        let remote_addr:String = matches.opt_str("r").unwrap();
        // let mut local = local_addr.split(":").collect::<Vec<&str>>();
        // let mut remote = remote_addr.split(":").collect::<Vec<&str>>();
        let protocol = matches.opt_str("p").unwrap().to_uppercase();
        if matches.opt_present("d") {
            log::set_max_level(LevelFilter::Debug);
        }
        if protocol == "UDP"{
            udp_proxy(&local_addr, &remote_addr).await.unwrap();
        }
    } else {
        usage(&program, &opts);
    }
}
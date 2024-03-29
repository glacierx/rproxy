﻿
use std::sync::Arc;
use futures::future::try_join;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::SystemTime;
use std::io::ErrorKind::Other;
use tokio::net::UdpSocket;
use tokio::time;
use log::*;
use futures::StreamExt;
use futures::{
    channel::{
        mpsc::{
            UnboundedReceiver,
            UnboundedSender,
            unbounded
        }
    }
};

use crate::dns::DNSResolve;

#[allow(dead_code)]
enum MessageType{
    Data,
    Terminate,
    DNS
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

    async fn run(mut self) -> Result<(), std::io::Error>{
        let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await.unwrap());
        // let (mut socket_recv, mut socket_send) = socket.split();
        let socket_recv = socket.clone();
        let socket_send = socket.clone();
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
                    // debug!("Forward {} bytes from {}", buf.len(), _peer);

                    match socket_send.send_to(&buf[..], &remote_addr).await {
                        Ok(_sz) => {

                        },
                        Err(_e) => {
                            panic!("{}", _e);
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
                tokio::select! {
                    x = socket_recv.recv_from(&mut buf) => {
                        if let Ok((_size, _peer)) = x {
                            // debug!("Recv {} bytes to {}", _size, client_peer);
                            match _tx.unbounded_send((client_peer, Vec::from(&buf[.._size]), MessageType::Data)) {
                                Ok(_sz) => {
        
                                },
                                Err(_e) => {
                                    return Err(std::io::Error::from(Other));
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
    dns: Vec<String>,
    client_tunnels: HashMap<SocketAddr, (Tx, SystemTime)>

}

impl<'a> DNSResolve<'a> for UDPProxy<'a>{
    fn remote(&self) ->  &String {
        &self.remote
    }

    fn dns(&self) ->  &Vec<String>  {
        &self.dns
    }

    fn reset_dns(&mut self,d: &Vec<String>) -> usize {
        self.dns = d.clone();
        d.len()
    }

}


impl<'a> UDPProxy<'a> {

    async fn run(&mut self) -> Result<(), std::io::Error> {
        let socket = Arc::new(UdpSocket::bind(&self.addr).await.unwrap());
        info!("Listening on {}", socket.local_addr().unwrap());
        // let server: Vec<_> = self.remote
        //                     .to_socket_addrs()
        //                     .expect("Unable to resolve domain")
        //                     .collect();

        // let _remote = server[0];
        self.resolve().await.unwrap();
        let mut dns_timeout = time::interval(tokio::time::Duration::from_secs(30));
        let mut _remote = self.dns[0].clone();
        // let (mut socket_recv, mut socket_send) = socket.split();
        let socket_recv = socket.clone();
        let socket_send = socket.clone();
        let (tx, mut rx) = unbounded::<(SocketAddr, Vec<u8>,  MessageType)>();
        let remote_to_client_proc = async move {
            loop{
                if let Some((peer, buf, _msg_type)) = rx.next().await {
                    // debug!("Forward {} bytes to {}", buf.len(), peer);
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
            // let mut client_tunnels:HashMap<SocketAddr, (Tx, SystemTime)> = HashMap::new();
            let mut time_out1 = time::interval(tokio::time::Duration::from_secs(5));
            loop{

                tokio::select! {
                    data = socket_recv.recv_from(&mut buf) => {
                        match data {
                            Ok((size, peer)) => {
                                // let _addr = format!("{}:{}", peer.ip(), peer.port());
                                match self.client_tunnels.get(&peer) {
                                    Some((_tx, _active_time)) => {
                                        // _tx.unbounded_send((peer, Vec::from(&buf[..size]), MessageType::Data)).unwrap();
                                    },
                                    _ => {
                                        info!("New client {}:{} is added", peer.ip(), peer.port());
                                        let (mut _s,_r) = unbounded::<(SocketAddr, Vec<u8>,  MessageType)>();
                                        // _s.unbounded_send((peer, buf.clone(), MessageType::Data)).unwrap();
                                        self.client_tunnels.insert(peer, (_s, SystemTime::now()));
                                        let c = UDPPeerPair {
                                            client : peer,
                                            remote: _remote.parse::<SocketAddr>().unwrap(),
                                            send: tx.clone(),
                                            recv: _r
                                        };
                                        tokio::spawn(c.run());
                                    }
                                }
                                let (tx, tm) = &mut self.client_tunnels.get_mut(&peer).unwrap();
                                // debug!("Recv {} bytes from {}", size, peer);
                                tx.unbounded_send((peer, Vec::from(&buf[..size]), MessageType::Data)).unwrap();
                                *tm = SystemTime::now();
                            },
                            Err(e) => {
                                warn!("recv_from {:?} returned error {}, {:?}", socket_recv, e, e);
                                break;
                            }
                        }
                    },
                    _ = dns_timeout.tick() => {
                        self.resolve().await.unwrap();
                        _remote = self.dns[0].clone();
                    },
                    _ = time_out1.tick() =>{
                        debug!("Tick");
                        let mut tbd: Vec<SocketAddr> = Vec::new();
                        for (k, v) in (&mut self.client_tunnels).iter(){
                            let sec = v.1.elapsed().unwrap().as_secs();
                            if sec > 120{
                                info!("Client {}:{} is timeout({}s)", k.ip(), k.port(), sec);
                                v.0.unbounded_send((k.clone(), empty.clone(), MessageType::Terminate)).unwrap();
                                tbd.push(k.to_owned());
                            } else {
                                debug!("Client {}:{} is good. ({}s)", k.ip(), k.port(), sec);
                            }
                        }

                        for k in tbd{
                            self.client_tunnels.remove(&k);
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

pub async fn udp_proxy(local: &String, 
    remote:&String) -> Result<(), std::io::Error>
{
    let mut server = UDPProxy {
        addr: &local,
        remote: &remote,
        dns: vec![],
        client_tunnels: HashMap::new()
    };
    info!("Start service in UDP mode {}->{}", server.addr, server.remote);
    return server.run().await;
}
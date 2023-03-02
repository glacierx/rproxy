use std::collections::HashSet;
use std::io::Error;

use futures::future::try_join;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::TcpListener;
use tokio::net::lookup_host;
use log::*;
use tokio::time;

struct TCPPeerPair {
    client: TcpStream,
    remote: String,
}

impl TCPPeerPair {
    async fn run(mut self) -> Result<(), std::io::Error>{
        let mut outbound = TcpStream::connect(self.remote.clone()).await?;

        let (mut ri, mut wi) = self.client.split();
        let (mut ro, mut wo) = outbound.split();
    
        let client_to_server = async {
            tokio::io::copy(&mut ri, &mut wo).await?;
            wo.shutdown().await
        };
    
        let server_to_client = async {
            tokio::io::copy(&mut ro, &mut wi).await?;
            wi.shutdown().await
        };
    
        try_join(client_to_server, server_to_client).await?;
        Ok(())
    }
}

struct TCPProxy<'a> {
    addr: &'a String,
    remote: &'a String,
    dns: Vec<String>
}


impl<'a> TCPProxy<'a> {
    fn domain(&self) -> String {
        let mut s = self.remote.split(":");
        s.next().unwrap().to_string()
    }
    fn port(&self) -> Option<u16> {
        let s: Vec<&str> = self.remote.split(":").collect();
        if s.len() > 1{
            let p = s.last().unwrap();
            Some(p.parse::<u16>().unwrap())
        }else {
            None
        }
    }
    async fn resolve(& mut self) -> Result<usize, Error> { 
        let socka = lookup_host(self.remote).await;
        match socka {
            Ok(sk) => {
                let mut last = HashSet::new();
                last.extend(self.dns.clone());
                self.dns.clear();
                for addr in sk{
                    self.dns.push(addr.to_string());
                    let d = self.dns.last().unwrap();
                    if !last.contains(d){
                        info!("Resolve {} -> {}", self.remote, d);
                    }
                }
            },
            Err(e) => {
                warn!("{:?}", e);
                self.dns = vec![self.domain()];
            }
        }
        Ok(self.dns.len())
    }

    async fn run(& mut self) -> Result<(), std::io::Error> {
        self.resolve().await.unwrap();
        let mut time_out1 = time::interval(tokio::time::Duration::from_secs(30));
        let mut host = self.dns[0].clone();
        match TcpListener::bind(self.addr).await {
            Ok(listener) => {
                loop{
                    tokio::select!{
                        x = listener.accept() => {
                            match x {
                                Ok((inbound, _)) => {
                                    // let transfer = Self::transfer(inbound, self.remote.clone());
                                    // tokio::spawn(transfer);
                                    let client = TCPPeerPair{
                                        client: inbound,
                                        remote: host.clone()
                                    };
                                    tokio::spawn(client.run());
                                },
                                Err(e1) => {
                                    error!("Failed to accept new connection from {}, err={:?}", self.addr, e1);
                                }
                            }                            
                        },
                        _ = time_out1.tick() => {
                            self.resolve().await.unwrap();
                            host = self.dns[0].clone();
                        }
                    }
                }
            },
            Err(e) => {
                error!("Failed to bind interface {}, err={:?}", self.addr, e);
            }
        }

        Ok(())
    }
}

pub async fn tcp_proxy(local: &String, 
    remote:&String) -> Result<(), std::io::Error>
{
    let mut server = TCPProxy {
        addr: &local,
        remote: &remote,
        dns: vec![]
    };
    info!("Start service in TCP mode {}->{}", server.addr, server.remote);
    return server.run().await;
}
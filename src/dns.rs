use std::collections::HashSet;
use std::io::Error;
use tokio::net::lookup_host;
use log::*;
use async_trait::async_trait;

#[async_trait]
#[allow(unused_variables)]
pub trait DNSResolve <'a> {
    fn remote(&self) -> &String;
    fn dns(& self) -> &Vec<String>;
    fn reset_dns(&mut self, d: &Vec<String> ) -> usize;
    
    fn domain(&self) -> String {
        let mut s = self.remote().split(":");
        s.next().unwrap().to_string()
    }
    
    fn port(&self) -> Option<u16> {
        let s: Vec<&str> = self.remote().split(":").collect();
        if s.len() > 1{
            let p = s.last().unwrap();
            Some(p.parse::<u16>().unwrap())
        }else {
            None
        }
    }
    fn on_dns_changed(&self, new_addr: &String) {

    }

    async fn resolve(& mut self) -> Result<usize, Error> { 
        let remote = self.remote().clone();
        let res:usize;
        let socka = lookup_host(remote.clone()).await;
        match socka {
            Ok(sk) => {
                let mut last = HashSet::new();
                last.extend(self.dns().clone());
                let mut dns = vec![];
                for addr in sk{
                    let d = addr.to_string();
                    if !last.contains(&d){
                        info!("Resolve {} -> {}", remote, d);
                        self.on_dns_changed(&d);
                    }
                    dns.push(d);
                }
                res = self.reset_dns(&dns);
            },
            Err(e) => {
                warn!("{:?}", e);
                let d = vec![self.domain()];
                res = self.reset_dns(&d);
            }
        }
        Ok(res)
    }    
}



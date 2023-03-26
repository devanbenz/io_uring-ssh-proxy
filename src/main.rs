use std::{rc::Rc, sync::{Arc, atomic::{AtomicU64, Ordering}}};

use tokio_uring::{net::{TcpListener, TcpStream}, buf::IoBuf};

fn main() {
   tokio_uring::start(uring_fn()); 
}

async fn uring_fn() {
    env_logger::init();

    let num_conns: Arc<AtomicU64> = Default::default();

    let addr = "0.0.0.0:8080".parse().unwrap();

    let listener = TcpListener::bind(addr).unwrap();
    while let Ok((ingress, _)) = listener.accept().await {
        println!("Accepted connection!");

        let num_conns = num_conns.clone();
        tokio_uring::spawn(async move {
            let egress_addr = "127.0.0.2:22".parse().unwrap();
            let egress = TcpStream::connect(egress_addr).await.unwrap();

            num_conns.fetch_add(1, Ordering::SeqCst);
            log::debug!("{num_conns:?} connection(s) added.");

            let egress =  Rc::new(egress);
            let ingress = Rc::new(ingress);

            let mut ingress_copy = tokio_uring::spawn(copy(ingress.clone(), egress.clone()));
            let mut egress_copy = tokio_uring::spawn(copy(egress.clone(), ingress.clone()));

            let res = tokio::try_join!(&mut ingress_copy, &mut egress_copy);
            if let Err(e) = res {
                log::error!("Error with uring i/o: {}", e);
            }

            ingress_copy.abort();
            egress_copy.abort();

            num_conns.fetch_sub(1, Ordering::SeqCst);
            log::debug!("Connection(s) dropped. Current connections: {num_conns:?}.");
        });
    }   
}

async fn copy(from: Rc<TcpStream>, to: Rc<TcpStream>) -> Result<(), std::io::Error> {
    let mut buf = vec![0u8; 1024];
    loop {
        let (res, buf_read) = from.read(buf).await;

        let n = res?;
        if n == 0 {
            return Ok(());
        }

        let (res, buf_write) = to.write(buf_read.slice(..n)).await;
        res?;

        buf = buf_write.into_inner();
    }
}

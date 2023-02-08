// Copyright Istio Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::net::SocketAddr;
use std::net::{IpAddr, Ipv6Addr};
use std::{cmp, io};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;
use tracing::{debug, info};

#[derive(Copy, Clone, Debug)]
pub enum Mode {
    ReadWrite,
    Write,
    Read,
}

/// run_client reads and/or writes data as fast as possible
pub async fn run_client(
    stream: &mut TcpStream,
    target: usize,
    mode: Mode,
) -> Result<(), io::Error> {
    let mut buf = vec![0; cmp::min(BUFFER_SIZE, target)];
    let start = Instant::now();
    let (mut r, mut w) = stream.split();
    let mut transferred = 0;
    while transferred < target {
        let length = cmp::min(buf.len(), target - transferred);
        match mode {
            Mode::ReadWrite => {
                let written = w.write(&buf[..length]).await?;
                transferred += written;
                r.read_exact(&mut buf[..written]).await?;
            }
            Mode::Write => {
                transferred += w.write(&buf[..length]).await?;
            }
            Mode::Read => {
                transferred += r.read(&mut buf[..length]).await?;
            }
        }
        debug!(
            "throughput: {:.3} Gb/s, transferred {} Gb ({:.3}%) in {:?} ({:?})",
            transferred as f64 / (start.elapsed().as_micros() as f64 / 1_000_000.0) / 0.125e9,
            transferred as f64 / 0.125e9,
            100.0 * transferred as f64 / target as f64,
            start.elapsed(),
            mode
        );
    }
    let elapsed = start.elapsed().as_micros() as f64 / 1_000_000.0;
    let throughput = transferred as f64 / elapsed / 0.125e9;
    info!(
        "throughput: {:.3} Gb/s, transferred {transferred} in {:?} ({:?})",
        throughput,
        start.elapsed(),
        mode
    );
    Ok(())
}

pub struct TestServer {
    listener: TcpListener,
    mode: Mode,
}

static BUFFER_SIZE: usize = 2 * 1024 * 1024;

impl TestServer {
    pub async fn new(mode: Mode) -> TestServer {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(addr).await.unwrap();
        TestServer { listener, mode }
    }

    pub fn address(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }

    pub async fn run(self) {
        loop {
            let (mut socket, _) = self.listener.accept().await.unwrap();

            tokio::spawn(async move {
                match self.mode {
                    Mode::ReadWrite => {
                        let (mut r, mut w) = socket.split();
                        let mut r = tokio::io::BufReader::with_capacity(BUFFER_SIZE, &mut r);
                        tokio::io::copy_buf(&mut r, &mut w).await.expect("tcp copy");
                    }
                    Mode::Write => {
                        let buffer = vec![0; BUFFER_SIZE];
                        loop {
                            let read = socket.write(&buffer).await.expect("tcp ready");
                            if read == 0 {
                                break;
                            }
                        }
                    }
                    Mode::Read => {
                        let mut buffer = vec![0; BUFFER_SIZE];
                        loop {
                            let read = socket.read(&mut buffer).await.expect("tcp ready");
                            if read == 0 {
                                break;
                            }
                        }
                    }
                }
            });
        }
    }
}

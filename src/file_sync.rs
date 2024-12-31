use anyhow::{Context, Result};
use core::str;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::{Duration, SystemTime},
};

#[macro_export]
macro_rules! dbgln {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!($($arg)*);
    };
}

pub struct FileSyncServer {
    socket: UdpSocket,
    source: SocketAddr,
}

impl FileSyncServer {
    pub fn broadcast() -> Result<Self> {
        // Start listening on UDP for "Expression of interest" broadcasts
        dbgln!("discovery: waiting for expression of interest...");
        let udp = std::net::UdpSocket::bind("0.0.0.0:6967")
            .context("failed to bind server discovery udp socket (0.0.0.0:6967)")?;
        let mut eoi_buf = vec![0; 3];
        let source;
        loop {
            let Ok((n_bytes, src_addr)) = udp.recv_from(&mut eoi_buf) else {
                continue;
            };

            // if a packet is an expression of interest
            if n_bytes == 3 && eoi_buf[0] == b'E' && eoi_buf[1] == b'O' && eoi_buf[2] == b'I' {
                source = src_addr;
                break;
            }
        }
        dbgln!("discovery: recieved expression of interest ({source:?})");
        Ok(Self {
            socket: udp,
            source,
        })
    }

    pub fn transfer(&self, name: &str, mut stream: impl Read + Seek) -> Result<()> {
        let file_name_bytes = name.as_bytes();
        let server = TcpListener::bind("0.0.0.0:6968")
            .context("failed to bind server transfer tcp socket (0.0.0.0:6968)")?;
        dbgln!(
            "server: bound ({})",
            server
                .local_addr()
                .map(|x| x.to_string())
                .unwrap_or(String::from("can't resolve local address"))
        );

        dbgln!(
            "discovery: sending acknowledgement of interest ({:?})",
            self.source
        );
        // Send ack after the server has started
        let ack_buf = vec![b'A', b'C', b'K'];
        self.socket
            .send_to(&ack_buf, self.source)
            .context("failed to send acklowledgement udp packet.")?;

        let mut client = server
            .accept()
            .context("failed to send acklowledgement udp packet.")?;
        dbgln!("server: client connected ({:?})", client.1);
        dbgln!("transfer: sending headers for {name}...");

        // First, write the size of the file name, then write the file name
        client
            .0
            .write_all(&[file_name_bytes.len() as u8])
            .context("failed to send header file name size packet.")?;
        client
            .0
            .write_all(file_name_bytes)
            .context("failed to send header file name data packet.")?;

        // Secondly, split the file into 10mb chunks, then send each 10mb chunk over tcp
        let mut len: i128 = stream
            .seek(SeekFrom::End(0))
            .context("failed to reach end of file/determine file size.")?
            .into();
        let og_len = len;
        stream
            .seek(SeekFrom::Start(0))
            .context("error: failed to reach start of file")?;

        // Now, dedicate 8 bytes to write the size of the file
        client
            .0
            .write_all(&og_len.to_be_bytes())
            .context("failed to write file size.")?;

        let mut buf = vec![0; 10_000_000];
        let mut last_notified = SystemTime::now();

        while len > 0 {
            let read_count = stream
                .read(&mut buf)
                .context("failed to read chunk from file.")?;
            if read_count == 0 {
                continue;
            }
            len -= read_count as i128;

            client
                .0
                .write_all(&buf[..read_count])
                .context("failed to write file chunk to transfer stream.")?;
            // Get percentage of file which remains to be sent
            let to_be_sent_perc = len as f64 / og_len as f64;
            let progress = (1.0 - to_be_sent_perc) * 100.0;
            let now = SystemTime::now();

            if len <= 0
                || now
                    .duration_since(last_notified)
                    .expect("last notified should occur before the present")
                    > Duration::from_millis(200)
            {
                last_notified = now;
                dbgln!(
                    "transfer: {name}: ({progress:.02}%, {} bytes)",
                    og_len - len
                );
            }
        }
        Ok(())
    }
}

pub struct FileSyncClient {
    server: TcpStream,
    name: String,
    size: i128,
}

impl FileSyncClient {
    pub fn broadcast() -> Result<Self> {
        // Broadcast expression of interest
        dbgln!("discovery: broadcasting expression of interest...");

        let mut ack_buf = vec![0; 3];
        let mut server;
        loop {
            let udp = UdpSocket::bind("0.0.0.0:6966")
                .context("failed to bind discovery udp broadcast.")?;
            udp.set_broadcast(true)
                .expect("udp socket should be allowed to enable broadcast");
            udp.send_to(b"EOI", "255.255.255.255:6967")
                .context("failed to send expression of interest broadcast.")?;
            udp.set_read_timeout(Some(Duration::from_millis(2000)))
                .expect("should be able to set udp read timeout");

            let Ok((count, sender)) = udp.recv_from(&mut ack_buf) else {
                continue;
            };

            if count == 3 && ack_buf == b"ACK" {
                server = sender;
                dbgln!("discovery: expression of interest acknowledged ({server:?})");
                break;
            }
        }
        server.set_port(6968);

        dbgln!("client: attempting connection ({server:?})...");
        let mut server = TcpStream::connect(server).unwrap_or_else(|e| {
            eprintln!("error: failed to connect to tcp transfer server on {server:?}. {e}");
            std::process::exit(1);
        });
        dbgln!("client: recieving headers for file...");
        // Firstly, we are expecting the size of the file name
        let mut size = [0; 1];
        server
            .read_exact(&mut size)
            .context("failed to read file name size.")?;
        let mut name = vec![0; size[0] as usize];
        server
            .read_exact(&mut name)
            .context("failed to read file name data (len={size:?}).")?;
        let mut size_buf = vec![0; 16];
        server
            .read_exact(&mut size_buf)
            .context("failed to read file size.")?;
        let size = i128::from_be_bytes(size_buf.try_into().expect("Failed to convert bytes"));
        let name = str::from_utf8(&name[..]).unwrap_or_else(|e| {
            eprintln!("error: failed to read file name ({name:?}). {e}");
            std::process::exit(1);
        });

        dbgln!("client: {name} ({size} bytes)");
        Ok(Self {
            server,
            name: name.to_string(),
            size,
        })
    }

    pub fn name_and_size(&self) -> (&str, usize) {
        (&self.name, self.size as usize)
    }

    pub fn recieve(&mut self, mut writer: impl Write) -> Result<()> {
        let mut buf = vec![0; 10_000_000];
        let mut fsize: usize = 0;

        let mut last_notified = SystemTime::now();
        loop {
            let count = self.server.read(&mut buf).context("error before EOF")?;
            if count == 0 {
                break;
            }

            writer
                .write(&buf[..count])
                .context("failed to write chunk to disk.")?;
            fsize += count;

            // Get downloaded percentage
            let perc = fsize as f64 / self.size as f64;
            let now = SystemTime::now();
            if fsize as i128 == self.size
                || now
                    .duration_since(last_notified)
                    .expect("last notified should be before the present")
                    > Duration::from_millis(200)
            {
                last_notified = now;
                dbgln!(
                    "transfer: {} ({:.02}%, {fsize} bytes)",
                    self.name,
                    perc * 100.0
                );
            }
        }
        Ok(())
    }
}

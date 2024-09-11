use core::str;
use std::{
    fs::File,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{Duration, SystemTime},
};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() == 1 {
        // Start listening on UDP for "Expression of interest" broadcasts
        println!("discovery: waiting for expression of interest...");
        let udp = std::net::UdpSocket::bind("0.0.0.0:6967").unwrap_or_else(|e| {
            eprintln!("error: failed to bind server discovery udp socket (0.0.0.0:6967): {e}");
            std::process::exit(1);
        });
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
        println!("discovery: recieved expression of interest ({source:?})");

        let mut file = std::fs::File::open(&args[0]).unwrap_or_else(|e| {
            eprintln!("error: failed to open file: {}. {e}", &args[0]);
            std::process::exit(1);
        });
        let file_name = std::path::Path::new(&args[0])
            .file_name()
            .unwrap_or_else(|| {
                eprintln!(
                    "error: failed to get file name from input file: {}",
                    &args[0]
                );
                std::process::exit(1);
            })
            .to_str()
            .unwrap_or_else(|| {
                eprintln!(
                    "error: failed to convert the name from input file: {}",
                    &args[0]
                );
                std::process::exit(1);
            });

        let file_name_bytes = file_name.as_bytes();
        let server = TcpListener::bind("0.0.0.0:6968").unwrap_or_else(|e| {
            eprintln!("error: failed to bind server transfer tcp socket (0.0.0.0:6968): {e}");
            std::process::exit(1);
        });
        println!(
            "server: bound ({})",
            server
                .local_addr()
                .map(|x| x.to_string())
                .unwrap_or(String::from("can't resolve local address"))
        );

        println!("discovery: sending acknowledgement of interest ({source:?})");
        // Send ack after the server has started
        let ack_buf = vec![b'A', b'C', b'K'];
        udp.send_to(&ack_buf, source).unwrap_or_else(|e| {
            eprintln!("error: failed to send acklowledgement udp packet. {e}");
            std::process::exit(1);
        });

        let mut client = server.accept().unwrap_or_else(|e| {
            eprintln!("error: failed to send acklowledgement udp packet. {e}");
            std::process::exit(1);
        });
        println!("server: client connected ({:?})", client.1);
        println!("transfer: sending headers for {file_name}...");

        // First, write the size of the file name, then write the file name
        client
            .0
            .write_all(&[file_name_bytes.len() as u8])
            .unwrap_or_else(|e| {
                eprintln!("error: failed to send header file name size packet. {e}");
                std::process::exit(1);
            });
        client.0.write_all(file_name_bytes).unwrap_or_else(|e| {
            eprintln!("error: failed to send header file name data packet. {e}");
            std::process::exit(1);
        });

        // Secondly, split the file into 10mb chunks, then send each 10mb chunk over tcp
        let mut len: i128 = file.seek(SeekFrom::End(0)).unwrap_or_else(|e| {
            eprintln!("error: failed to reach end of file/determine file size. {e}");
            std::process::exit(1);
        }) as i128;
        let og_len = len;
        file.seek(SeekFrom::Start(0)).unwrap_or_else(|e| {
            eprintln!("error: failed to reach start of file. {e}");
            std::process::exit(1);
        });

        // Now, dedicate 8 bytes to write the size of the file
        client
            .0
            .write_all(&og_len.to_be_bytes())
            .unwrap_or_else(|e| {
                eprintln!("error: failed to write file size. {e}");
                std::process::exit(1);
            });

        let mut buf = vec![0; 10_000_000];
        let mut last_notified = SystemTime::now();

        while len > 0 {
            let read_count = file.read(&mut buf).unwrap_or_else(|e| {
                eprintln!("error: failed to read chunk from file. {e}");
                std::process::exit(1);
            });
            if read_count == 0 {
                continue;
            }
            len -= read_count as i128;

            client.0.write_all(&buf[..read_count]).unwrap_or_else(|e| {
                eprintln!("error: failed to write file chunk to transfer stream. {e}");
                std::process::exit(1);
            });
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
                println!(
                    "transfer: {file_name}: ({progress:.02}%, {} bytes)",
                    og_len - len
                );
            }
        }
    } else {
        // Broadcast expression of interest
        println!("discovery: broadcasting expression of interest...");

        let mut ack_buf = vec![0; 3];
        let mut server;
        loop {
            let udp = UdpSocket::bind("0.0.0.0:6966").unwrap_or_else(|e| {
                eprintln!("error: failed to bind discovery udp broadcast. {e}");
                std::process::exit(1);
            });
            udp.set_broadcast(true)
                .expect("udp socket should be allowed to enable broadcast");
            udp.send_to(&[b'E', b'O', b'I'], "255.255.255.255:6967")
                .unwrap_or_else(|e| {
                    eprintln!("error: failed to send expression of interest broadcast. {e}");
                    std::process::exit(1);
                });
            udp.set_read_timeout(Some(Duration::from_millis(2000)))
                .expect("should be able to set udp read timeout");

            let Ok((count, sender)) = udp.recv_from(&mut ack_buf) else {
                continue;
            };

            if count == 3 && ack_buf == &[b'A', b'C', b'K'] {
                server = sender;
                println!("discovery: expression of interest acknowledged ({server:?})");
                break;
            }
        }
        server.set_port(6968);

        println!("client: attempting connection ({server:?})...");
        let mut server = TcpStream::connect(server).unwrap_or_else(|e| {
            eprintln!("error: failed to connect to tcp transfer server on {server:?}. {e}");
            std::process::exit(1);
        });
        println!("client: recieving headers for file...");
        // Firstly, we are expecting the size of the file name
        let mut size = [0; 1];
        server.read_exact(&mut size).unwrap_or_else(|e| {
            eprintln!("error: failed to read file name size. {e}");
            std::process::exit(1);
        });
        let mut name = vec![0; size[0] as usize];
        server.read_exact(&mut name).unwrap_or_else(|e| {
            eprintln!("error: failed to read file name data (len={size:?}). {e}");
            std::process::exit(1);
        });
        let mut size_buf = vec![0; 16];
        server.read_exact(&mut size_buf).unwrap_or_else(|e| {
            eprintln!("error: failed to read file size. {e}");
            std::process::exit(1);
        });
        let size = i128::from_be_bytes(size_buf.try_into().unwrap_or_else(|e| {
            eprintln!("error: failed to convert file size bytes. {e:?}");
            std::process::exit(1);
        }));
        let name = str::from_utf8(&name[..]).unwrap_or_else(|e| {
            eprintln!("error: failed to read file name ({name:?}). {e}");
            std::process::exit(1);
        });

        println!("client: {name} ({size} bytes)");
        let file = File::create(&name[..]).unwrap_or_else(|e| {
            eprintln!("error: failed to create file ({name}). {e}");
            std::process::exit(1);
        });
        let mut writer = BufWriter::new(file);
        let mut buf = vec![0; 10_000_000];
        let mut fsize: usize = 0;

        let mut last_notified = SystemTime::now();
        loop {
            let Ok(count) = server.read(&mut buf) else {
                eprintln!("error before EOF");
                break;
            };
            if count == 0 {
                break;
            }

            writer.write(&buf[..count]).unwrap_or_else(|e| {
                eprintln!("error: failed to write chunk to disk. {e}");
                std::process::exit(1);
            });
            fsize += count;

            // Get downloaded percentage
            let perc = fsize as f64 / size as f64;
            let now = SystemTime::now();
            if fsize as i128 == size
                || now
                    .duration_since(last_notified)
                    .expect("last notified should be before the present")
                    > Duration::from_millis(200)
            {
                last_notified = now;
                println!("transfer: {name} ({:.02}%, {fsize} bytes)", perc * 100.0);
            }
        }
    }
}

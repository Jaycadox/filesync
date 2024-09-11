use core::str;
use std::{
    fs::File,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    time::Duration,
};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() == 1 {
        // Start listening on UDP for "Expression of interest" broadcasts
        println!("waiting for expression of interest...");
        let udp = std::net::UdpSocket::bind("0.0.0.0:6967").unwrap();
        let mut eoi_buf = vec![0; 3];
        let source;
        loop {
            let (n_bytes, src_addr) = udp.recv_from(&mut eoi_buf).unwrap();
            // if a packet is an expression of interest
            if n_bytes == 3 && eoi_buf[0] == b'E' && eoi_buf[1] == b'O' && eoi_buf[2] == b'I' {
                source = src_addr;
                break;
            }
        }
        println!("recieved expression of interest from: {source:?}, sending ack...");

        let mut file = std::fs::File::open(&args[0]).unwrap();
        let file_name = std::path::Path::new(&args[0])
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let file_name_bytes = file_name.as_bytes();
        let server = TcpListener::bind("192.168.0.110:6968").unwrap();

        // Send ack after the server has started
        let ack_buf = vec![b'A', b'C', b'K'];
        udp.send_to(&ack_buf, source).unwrap();

        let mut client = server.accept().unwrap();
        println!("Sending: {file_name}...");

        // First, write the size of the file name, then write the file name
        client.0.write_all(&[file_name_bytes.len() as u8]).unwrap();
        client.0.write_all(file_name_bytes).unwrap();

        // Secondly, split the file into 10mb chunks, then send each 10mb chunk over tcp
        let mut len: i128 = file.seek(SeekFrom::End(0)).unwrap() as i128;
        file.seek(SeekFrom::Start(0)).unwrap();

        let mut buf = vec![0; 10_000_000];
        while len > 0 {
            let read_count = file.read(&mut buf).unwrap();
            if read_count == 0 {
                continue;
            }
            len -= read_count as i128;
            client.0.write_all(&buf[..read_count]).unwrap();
            println!("Len = {len}");
        }
    } else {
        // Broadcast expression of interest
        println!("broadcasting expression of interest...");

        let mut ack_buf = vec![0; 3];
        let mut server;
        loop {
            let udp = UdpSocket::bind("0.0.0.0:6966").unwrap();
            udp.set_broadcast(true).unwrap();
            udp.send_to(&[b'E', b'O', b'I'], "255.255.255.255:6967")
                .unwrap();
            udp.set_read_timeout(Some(Duration::from_millis(2000)))
                .unwrap();

            let Ok((count, sender)) = udp.recv_from(&mut ack_buf) else {
                continue;
            };

            if count == 3 && ack_buf == &[b'A', b'C', b'K'] {
                server = sender;
                println!(
                    "expression of interest acknowledged by: {server:?}, attempting connection..."
                );
                break;
            }
        }
        server.set_port(6968);

        let mut server = TcpStream::connect(server).unwrap();
        // Firstly, we are expecting the size of the file name
        let mut size = [0; 1];
        server.read_exact(&mut size).unwrap();
        let mut name = vec![0; size[0] as usize];
        server.read_exact(&mut name).unwrap();
        println!("downloading file: {}", str::from_utf8(&name[..]).unwrap());
        let file = File::create(str::from_utf8(&name[..]).unwrap()).unwrap();
        let mut writer = BufWriter::new(file);
        let mut buf = vec![0; 10_000_000];
        let mut fsize: usize = 0;
        loop {
            let Ok(count) = server.read(&mut buf) else {
                eprintln!("Error before EOF");
                break;
            };
            if count == 0 {
                break;
            }
            writer.write(&buf[..count]).unwrap();
            fsize += count;
        }
        println!("done (len = {})", fsize);
    }
}

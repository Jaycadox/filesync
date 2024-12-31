mod file_sync;
use std::{fs::File, io::BufWriter};

use file_sync::*;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() == 1 {
        let file = std::fs::File::open(&args[0]).unwrap_or_else(|e| {
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

        let server = FileSyncServer::broadcast().unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });
        server.transfer(file_name, file).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });
    } else {
        let mut client = FileSyncClient::broadcast().unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });
        let (name, _size) = client.name_and_size();
        let file = File::create(name).unwrap_or_else(|e| {
            eprintln!("error: failed to create file ({name}). {e}");
            std::process::exit(1);
        });
        let writer = BufWriter::new(file);
        client.recieve(writer).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });
    }
}

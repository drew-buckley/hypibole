use std::env;
use std::fs;
use std::process::{Command, Stdio};
use std::str::from_utf8;
use serde::{Deserialize};
use std::io::prelude::*;

#[derive(Deserialize)]
struct Config {
    network: Option<Network>,
    board: Option<Board>
}

#[derive(Deserialize)]
struct Network {
    address: Option<String>,
    port: Option<String>
}

#[derive(Deserialize)]
struct Board {
    gets: Option<String>,
    sets: Option<String>,
    simgets: Option<String>,
    simsets: Option<String>
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("Provide path to hypibole executable as first argument and configuration file as second argument.");
    }

    let config_file = args.get(2).unwrap();
    let config_data = fs::read_to_string(config_file)
        .expect(&format!("Unable to read \"{}\".", config_file));

    let config: Config = toml::from_str(&config_data)
        .expect("Failed to parse TOML.");

    let hypibole_executable = args.get(1).unwrap();
    
    let mut hypibole_cmd = Command::new(hypibole_executable);
    
    if let Some(network) = config.network { 
        if let Some(ip) = network.address {
            hypibole_cmd.arg("--address").arg(ip);
        };

        if let Some(port) = network.port {
            hypibole_cmd.arg("--port").arg(port);
        };
    };

    if let Some(board) = config.board { 
        if let Some(gets) = board.gets {
            hypibole_cmd.arg("--gets").arg(gets);
        };

        if let Some(sets) = board.sets {
            hypibole_cmd.arg("--sets").arg(sets);
        };

        if let Some(simgets) = board.simgets {
            hypibole_cmd.arg("--simgets").arg(simgets);
        };

        if let Some(simsets) = board.simsets {
            hypibole_cmd.arg("--simsets").arg(simsets);
        };
    };

    hypibole_cmd.stdout(Stdio::piped());
    hypibole_cmd.stderr(Stdio::piped());
    let hypibole_process = hypibole_cmd.spawn()
        .expect("Failed to spawn hypibole.");

    let mut hypibole_stdout = hypibole_process.stdout.unwrap();
    let mut hypibole_stderr = hypibole_process.stderr.unwrap();

    let mut buffer: [u8; 1024] = [0; 1024];
    loop {
        let bytes_read = hypibole_stdout.read(&mut buffer)
            .expect("Failed to read STDOUT.");

        let stdout_str = from_utf8(&buffer[0 .. bytes_read])
            .expect("Failed to decode STDOUT.");

        print!("{}", stdout_str);

        let bytes_read = hypibole_stderr.read(&mut buffer)
            .expect("Failed to read STDERR.");

        let stderr_str = from_utf8(&buffer[0 .. bytes_read])
            .expect("Failed to decode STDERR.");

        eprint!("{}", stderr_str);
    }
}

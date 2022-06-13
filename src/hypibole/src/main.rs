use std::{convert::Infallible, net::SocketAddr};
use std::borrow::Cow;
use hyper::{Body, Request, Response, Server, Uri};
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use std::result::Result;
use std::sync::Arc;
use std::collections::HashSet;
use std::collections::HashMap;
use std::collections::BTreeMap;
use clap::Parser;
use rppal::gpio::{Gpio};
use rppal::gpio::IoPin;
use rppal::gpio::Level;
use core::fmt::{Debug};
use serde::{Deserialize, Serialize};
use atomic_refcell::AtomicRefCell;

use form_urlencoded;

type GpioIndex = u8;

const PIN_INDEX_PARAM_STR: &'static str = "pin";
const OPERATION_PARAM_STR: &'static  str = "op";
const GET_PARAM_STR: &'static  str = "get";
const SET_PARAM_STR: &'static  str = "set";
const LEVEL_PARAM_STR: &'static  str = "level";
const HIGH_PARAM_VALUE_STR: &'static  str = "high";
const LOW_PARAM_VALUE_STR: &'static  str = "low";

pub trait DiscreteIO {
    fn get_state(&self) -> Level;
    fn set_state(&self, level: &Level);
}

pub struct LevelContainer {
    level: Level
}

impl LevelContainer {
    pub fn set(&mut self, level: &Level) {
        self.level = *level;
    }

    pub fn get(&self) -> Level {
        self.level
    }
}

pub struct SimulatedPin {
    level: AtomicRefCell<LevelContainer>
}

impl SimulatedPin {
    pub fn new() -> SimulatedPin {
        SimulatedPin { level: AtomicRefCell::new(LevelContainer{ level: Level::Low }) }
    }
}

impl DiscreteIO for SimulatedPin {
    fn get_state(&self) -> Level {
        self.level.borrow().get().clone()
    }

    fn set_state(&self, level: &Level) {
        self.level.borrow_mut().set(level);
    }
}

pub struct PhysicalPin {
    io_pin: AtomicRefCell<IoPin>
}

impl PhysicalPin {
    pub fn new(io_pin: IoPin) -> PhysicalPin {
        PhysicalPin { io_pin: AtomicRefCell::new(io_pin) }
    }
}

impl DiscreteIO for PhysicalPin {
    fn get_state(&self) -> Level {
        if self.io_pin.borrow().is_high() {
            Level::High
        }
        else {
            Level::Low
        }
    }

    fn set_state(&self, level: &Level) {
        if *level == Level::High {
            self.io_pin.borrow_mut().set_high();
        }
        else {
            self.io_pin.borrow_mut().set_low();
        }
    }
}

#[derive(Clone)]
struct PinSet {
    pub get_whitelist: HashSet<GpioIndex>,
    pub set_whitelist: HashSet<GpioIndex>,
    pub get_simulated: HashSet<GpioIndex>,
    pub set_simulated: HashSet<GpioIndex>,
}

#[derive(Clone)]
struct AppContext {
    pub pin_set: PinSet,
    pub physical_pin_map: Arc<HashMap<GpioIndex, PhysicalPin>>,
    pub simulated_pin_map: Arc<HashMap<GpioIndex, SimulatedPin>>
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    // Whitelist of gpio pin numbers to allow getting of state.
    #[clap(short, long, default_value = "")]
    gets: String,

    // Whitelist of gpio pin numbers to allow setting of state.
    #[clap(short, long, default_value = "")]
    sets: String,

    // IP address which to bind the server to.
    #[clap(short, long, default_value = "0.0.0.0")]
    address: String,

    // Listening port for the server.
    #[clap(short, long, default_value = "8080")]
    port: String,

    // Simulated gettable pins; real pins of the same index take priority.
    #[clap(long, default_value = "")]
    simgets: String,

    // Simulated settable pins; real pins of the same index take priority.
    #[clap(long, default_value = "")]
    simsets: String,
}

#[derive(PartialEq, Debug)]
enum Operation {
    Get(),
    Set(Level)
}

#[derive(PartialEq, Debug)]
struct OperationArgs {
    gpio_index: GpioIndex,
    operation: Operation
}

#[derive(Serialize, Deserialize)]
enum OperationStatus {
    Succeeded,
    Failed(String)
}

enum OperationResult {
    Get(OperationStatus, Level, GpioIndex),
    Set(OperationStatus, GpioIndex),
    Error(String)
}

#[tokio::main]
async fn main() 
{
    let args = Args::parse();
    
    let get_whitelist = parse_gpio_list(&args.gets)
        .expect("Error parsing gets list!");

    let set_whitelist = parse_gpio_list(&args.sets)
        .expect("Error parsing sets list!");

    let get_simulated = parse_gpio_list(&args.simgets)
        .expect("Error parsing simulated gets list!");

    let set_simulated = parse_gpio_list(&args.simsets)
        .expect("Error parsing simulated sets list!");

    let pin_set = PinSet { 
        get_whitelist: get_whitelist, 
        set_whitelist: set_whitelist, 
        get_simulated: get_simulated,
        set_simulated: set_simulated
    };

    let server_details = args.address + ":" + &args.port;

    let addr: SocketAddr = server_details
        .parse()
        .expect("Unable to parse socket address.");

    if let Err(e) = perform_service(&pin_set, &addr).await
    {
        eprintln!("Service error: {}", e);
    }
}

async fn perform_service(pin_set: &PinSet, addr: &SocketAddr) -> Result<(), String> {
    let mut physical_pin_map: HashMap<GpioIndex, PhysicalPin> = HashMap::new();
    if !pin_set.get_whitelist.is_empty() && !pin_set.set_whitelist.is_empty() {
        let gpio = match Gpio::new() {
            Ok(gpio) => gpio,
            Err(err) => return Err(err.to_string())
        };
        
        for gpio_index in &pin_set.set_whitelist {
            let pin = match gpio.get(*gpio_index) {
                Ok(pin) => pin,
                Err(err) => return Err(err.to_string())
            };

            let io_pin = pin.into_io(rppal::gpio::Mode::Output);
            physical_pin_map.insert(*gpio_index, PhysicalPin::new(io_pin));
        }

        for gpio_index in &pin_set.get_whitelist {
            if !pin_set.set_whitelist.contains(&gpio_index) {
                let pin = match gpio.get(*gpio_index) {
                    Ok(pin) => pin,
                    Err(err) => return Err(err.to_string())
                };

                let io_pin = pin.into_io(rppal::gpio::Mode::Input); 
                physical_pin_map.insert(*gpio_index, PhysicalPin::new(io_pin));
            }
        }
    }

    let mut simulated_pin_map: HashMap<GpioIndex, SimulatedPin> = HashMap::new();
    for gpio_index in &pin_set.set_simulated {
        simulated_pin_map.insert(*gpio_index, SimulatedPin::new());
    }

    for gpio_index in &pin_set.get_simulated {
        if !pin_set.set_whitelist.contains(&gpio_index) {
            simulated_pin_map.insert(*gpio_index, SimulatedPin::new());
        }
    }

    let physical_pin_map = Arc::new(physical_pin_map);
    let simulated_pin_map = Arc::new(simulated_pin_map);
    let make_service = make_service_fn(move |conn: &AddrStream| {
        let context = AppContext {
            pin_set: pin_set.clone(),
            physical_pin_map: physical_pin_map.clone(),
            simulated_pin_map: simulated_pin_map.clone()
        };

        let addr = conn.remote_addr();
        let service = service_fn(move |req| {
            handle(context.clone(), addr, req)
        });

        async move { Ok::<_, Infallible>(service) }
    });

    let server = Server::bind(&addr).serve(make_service);

    if let Err(e) = server.await 
    {
        return Err(e.to_string());
    }

    Ok(())
}

fn parse_gpio_list(gpio_list_str: &str) -> Result<HashSet<GpioIndex>, String> {
    let mut gpio_index_set: HashSet<GpioIndex> = HashSet::new();
    for substr in gpio_list_str.split(',') {
        if !substr.is_empty() {
            let gpio_index = match substr.parse::<GpioIndex>() {
                Ok(value) => value,
                Err(err) => return Err(err.to_string())
            };

            gpio_index_set.insert(gpio_index);
        }
    }
    
    Ok(gpio_index_set)
}



fn process_uri_into_operation(uri: &Uri) -> Result<OperationArgs, String> {
    let query_str = match uri.query() {
        Some(query_str) => query_str,
        None => return Err("No arguments in URL.".to_string())
    };

    let mut gpio_index_str: Option<Cow<str>> = None;
    let mut operation_str: Option<Cow<str>> = None;
    let mut level_str: Option<Cow<str>> = None;
    for query_pair in form_urlencoded::parse(query_str.as_bytes()) {
        let key = query_pair.0;
        let value = query_pair.1;

        match key.as_ref() {
            PIN_INDEX_PARAM_STR => gpio_index_str = Some(value), 
            OPERATION_PARAM_STR => operation_str = Some(value),
            LEVEL_PARAM_STR => level_str = Some(value),
            _ => return Err(format!("Unrecognized query parameter: \"{}\"", key.as_ref()))
        };
    }

    let gpio_index_str = gpio_index_str;
    let operation_str = operation_str;
    let level_str = level_str;

    let gpio_index = match gpio_index_str {
        Some(gpio_index_str) => match gpio_index_str.parse::<GpioIndex>() {
            Ok(gpio_index) => gpio_index,
            Err(e) => return Err(e.to_string())
        }
        None => return Err("Did not get required GPIO index argument.".to_string())
    };

    let op_args: OperationArgs = match operation_str {
        Some(operation_str) => match operation_str.as_ref() {
            GET_PARAM_STR => OperationArgs{ gpio_index: gpio_index, operation: Operation::Get() },
            SET_PARAM_STR => {
                let level = match level_str {
                    Some(level_str) => match level_str.as_ref() {
                        HIGH_PARAM_VALUE_STR => Level::High,
                        LOW_PARAM_VALUE_STR => Level::Low,
                        _ => return Err(format!("Unrecognized level parameter: \"{}\"", level_str.as_ref()))
                    }, 
                    None => return Err("Did not get level argument required for set.".to_string())
                };

                OperationArgs{ gpio_index: gpio_index, operation: Operation::Set(level) }
            },
            _ => return Err(format!("Unrecognized operation parameter: \"{}\"", operation_str.as_ref()))
        }
        None => return Err("Did not get required operation argument.".to_string())
    };

    Ok(op_args)
}

fn perform_board_operation(op_args: &OperationArgs, context: &AppContext) -> Result<OperationResult, String> {
    if let Some(pin) = context.physical_pin_map.get(&op_args.gpio_index) {
        return perform_pin_io(op_args, pin, &context.pin_set.get_whitelist, &context.pin_set.set_whitelist);
    }
    else if let Some(pin) = context.simulated_pin_map.get(&op_args.gpio_index) {
        return perform_pin_io(op_args, pin, &context.pin_set.get_simulated, &context.pin_set.set_simulated);
    }
    else {
        return Err(format!("Could not find pin {} in either map.", op_args.gpio_index))
    }
}

fn perform_pin_io(op_args: &OperationArgs, pin: &dyn DiscreteIO, get_whitelist: &HashSet<GpioIndex>, set_whitelist: &HashSet<GpioIndex>) -> Result<OperationResult, String> {
    match op_args.operation {
        Operation::Get() => {
            if get_whitelist.contains(&op_args.gpio_index) {
                let level = pin.get_state();
                return Ok(OperationResult::Get(OperationStatus::Succeeded, level, op_args.gpio_index));
            }
            else {
                return Err(format!("Pin, {}, is not in the get whitelist for this pin type!", op_args.gpio_index));
            }
        },
        Operation::Set(level) => {
            if set_whitelist.contains(&op_args.gpio_index) {
                pin.set_state(&level);
                return Ok(OperationResult::Set(OperationStatus::Succeeded, op_args.gpio_index));
            }
            else {
                return Err(format!("Pin, {}, is not in the set whitelist for this pin type!", op_args.gpio_index));
            }
        }
    }
}

fn level_to_str(level: &Level) -> String {
    match level {
        Level::High => "high".to_string(),
        Level::Low => "low".to_string()
    }
}

fn status_to_str(status: &OperationStatus) -> String {
    match status {
        OperationStatus::Succeeded => "success".to_string(),
        OperationStatus::Failed(message) => message.clone()
    }
}

fn generate_json_response(op_result: &OperationResult) -> Result<String, String> {
    let mut json_staging_set: BTreeMap<String, String> = BTreeMap::new();
    match op_result {
        OperationResult::Get(status, level, pin) => {
            json_staging_set.insert("operation".to_string(), "get".to_string());
            json_staging_set.insert("status".to_string(), status_to_str(status));
            json_staging_set.insert("level".to_string(), level_to_str(level));
            json_staging_set.insert("pin".to_string(), pin.to_string());
        }, 
        OperationResult::Set(status, pin) => {
            json_staging_set.insert("operation".to_string(), "set".to_string());
            json_staging_set.insert("status".to_string(), status_to_str(status));
            json_staging_set.insert("pin".to_string(), pin.to_string());
        },
        OperationResult::Error(e) => {
            json_staging_set.insert("error".to_string(), e.clone());
        }
    };

    let json_respose = match serde_json::to_string(&json_staging_set) {
        Ok(json) => json,
        Err(e) => return Err(e.to_string())
    };

    Ok(json_respose)
}

async fn handle(context: AppContext, _addr: SocketAddr, req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let operation_result: OperationResult;
    match process_uri_into_operation(req.uri()) {
        Ok(op_args) => {
            let op_result = match perform_board_operation(&op_args, &context) {
                Ok(op_result) => op_result,
                Err(e) => OperationResult::Error(format!("Failed to perform board operation: \"{}\"", e))
            };

            operation_result = op_result;
        },

        Err(e) => operation_result = OperationResult::Error(e.to_string())
    }

    let json_respose = match generate_json_response(&operation_result) {
        Ok(json) => json,
        Err(e) => format!("{{ \"Error\": \"{}\" }}", e.to_string())
    };

    Ok(Response::new(Body::from(json_respose)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::LinkedList;

    #[test]
    fn test_parse_gpio_list() {
        let expected_set: HashSet<GpioIndex> = HashSet::from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let test_str = "1,2,3,4,5,6,7,8,9,10";

        let result_set = parse_gpio_list(&test_str)
            .expect("parse_gpio_list function failed!");

        assert_eq!(expected_set, result_set);
    }

    #[test]
    fn test_process_uri_into_operation() {
        let mut test_map = HashMap::new();
        test_map.insert("http://hypibole.fun/?pin=1&op=get", OperationArgs { gpio_index: 1, operation: Operation::Get() });
        test_map.insert("http://hypibole.fun/?pin=2&op=get", OperationArgs { gpio_index: 2, operation: Operation::Get() });
        test_map.insert("http://hypibole.fun/?pin=3&op=set&level=high", OperationArgs { gpio_index: 3, operation: Operation::Set(Level::High) });
        test_map.insert("http://hypibole.fun/?pin=4&op=set&level=low", OperationArgs { gpio_index: 4, operation: Operation::Set(Level::Low) });

        let test_map = test_map;
        for test_entry in test_map {
            let uri = test_entry.0;
            let expected = &test_entry.1;
            let result = process_uri_into_operation(uri)
                .expect("process_uri_into_operation failure!");

            assert_eq!(result, *expected);
        }

        let bad_uris = LinkedList::from([
            "http://hypibole.fun/?pin=1&op=set",
            "http://hypibole.fun/?pin=string&op=set",
            "http://hypibole.fun/?pin=1&op=bigyawn",
            "http://hypibole.fun/?pin=1&op=set&level=somewhereinthemiddle",
            "This is not even close to a valid URI.",
        ]);

        for bad_uri in bad_uris {
            match process_uri_into_operation(bad_uri) {
                Ok(_) => panic!("These calls should never succeed!"),
                Err(e) => println!("Got expected error: \"{}\"", e)
            }
        }
    }

    #[test]
    fn test_simulated_pin() {
        let simulated_pin = SimulatedPin::new();
        for i in 0..100 {
            print!(".");
            let level = if i % 2 == 0 {
                Level::High
            }
            else {
                Level::Low
            };

            simulated_pin.set_state(&level);
            assert_eq!(level, simulated_pin.get_state());
        }
    }

}

//
// Plane Tracker
// Copyright (c) 2023-2024 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::Deg;
use crate::{data, data::ProgramData};
use gtk4 as gtk;
use gtk::{glib, glib::clone};
use pointing_utils::{LatLon, uom};
use std::{cell::RefCell, error::Error, rc::Rc, io::prelude::*};
use uom::{si::f64, si::{length, velocity}};

mod msg_type {
    pub const ES_IDENTIFICATION_AND_CATEGORY: i32 = 1;
    pub const ES_AIRBORNE_POSITION_MESSAGE: i32 = 3;
    pub const ES_AIRBORNE_VELOCITY_MESSAGE: i32 = 4;
    pub const SURVEILLANCE_ALT_MESSAGE: i32 = 5;
}

fn feet(value: f64) -> f64::Length {
    f64::Length::new::<length::foot>(value)
}

fn knots(value: f64) -> f64::Velocity {
    f64::Velocity::new::<velocity::knot>(value)
}

pub fn data_receiver(
    stream: std::net::TcpStream,
    rec_output: Option<std::fs::File>,
    sender: gtk::glib::Sender<data::SbsMessage>
) {
    let buf_reader = std::io::BufReader::new(stream);
    let mut buf_writer = if let Some(recording) = rec_output { Some(std::io::BufWriter::new(recording)) } else { None };

    for line in buf_reader.lines() {
        if let Ok(line) = line {
            if let Some(w) = &mut buf_writer {
                let _ = w.write(format!(
                    "{};{}\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.6f"),
                    line
                ).as_bytes()); //TODO: handle errors
            }

            match parse_sbs_message(&line) {
                Ok(m) => if let Some(m) = m { sender.send(m).unwrap(); },
                Err(e) => println!("Error parsing SBS1 message \"{}\": {}.", line, e)
            }
        }
    }
}

pub fn on_data_received(program_data_rc: &Rc<RefCell<ProgramData>>, msg: data::SbsMessage) {
    program_data_rc.borrow_mut().update(msg);
}

/// Returns `None` for unsupported message types.
fn parse_sbs_message(msg: &str) -> Result<Option<data::SbsMessage>, Box<dyn Error>> {
    let fields: Vec<&str> = msg.split(',').collect();

    if fields.is_empty() { return Err("empty message".into()); }

    if fields[0] != "MSG" { return Ok(None); }

    if fields.len() < 5 { return Err(format!("too few fields ({})", fields.len()).into()); }

    let msg_type = fields[1].parse::<i32>();

    if let Err(e) = msg_type { return Err(Box::new(e)); }

    if fields[4].is_empty() {
        return Err(format!("MSG,{} has empty field 5", msg_type::ES_IDENTIFICATION_AND_CATEGORY).into());
    }

    let id = fields[4].parse::<data::ModeSTransponderCode>()?;

    match msg_type.unwrap() {
        msg_type::ES_IDENTIFICATION_AND_CATEGORY => {
            if fields.len() < 11 {
                return Err(format!(
                    "MSG,{} has too few fields ({})",
                    msg_type::ES_IDENTIFICATION_AND_CATEGORY,
                    fields.len()).into()
                );
            }

            if fields[10].is_empty() {
                return Err(format!("MSG,{} has empty field 10", msg_type::ES_IDENTIFICATION_AND_CATEGORY).into());
            }

            return Ok(Some(data::SbsMessage::EsIdentificationAndCategory{
                id,
                callsign: fields[10].into()
            }));
        },

        msg_type::ES_AIRBORNE_POSITION_MESSAGE => {
            let altitude = match fields[11].parse::<u32>() {
                Ok(value) => Some(feet(value as f64)),
                _ => None
            };
            let lat = fields[14].parse::<f64>();
            let lon = fields[15].parse::<f64>();

            let lat_lon = match (lat, lon) {
                (Ok(lat), Ok(lon)) => Some(LatLon{ lat: Deg(lat), lon: Deg(lon) }),
                (Err(_), Err(_)) => None,
                (Ok(_), Err(e)) | (Err(e), Ok(_)) => return Err(Box::new(e))
            };

            return Ok(Some(data::SbsMessage::EsAirbornePosition{id, altitude, lat_lon}));
        },

        msg_type::ES_AIRBORNE_VELOCITY_MESSAGE => {
            let ground_speed = knots(fields[12].parse::<f64>()?);
            let track = Deg(fields[13].parse::<f64>()?);

            return Ok(Some(data::SbsMessage::EsAirborneVelocity{ id, ground_speed, track }));
        },

        msg_type::SURVEILLANCE_ALT_MESSAGE => {
            let altitude = feet(fields[11].parse::<u32>()? as f64);
            return Ok(Some(data::SbsMessage::SurveillanceAltitude{ id, altitude }));
        },

        _ => ()
    }

    Ok(None)
}

pub fn start(
    server_address: String,
    rec_output: Option<std::fs::File>,
    program_data_rc: &Rc<RefCell<ProgramData>>
) {
    let stream = std::net::TcpStream::connect(&server_address).unwrap();

    let (sender_worker, receiver_main) = glib::MainContext::channel(glib::Priority::DEFAULT);
    receiver_main.attach(None, clone!(@weak program_data_rc => @default-panic, move |msg| {
        on_data_received(&program_data_rc, msg);
        glib::ControlFlow::Continue
    }));

    let stream2 = stream.try_clone().unwrap();
    let worker = Some(std::thread::spawn(move || {
        data_receiver(stream2, rec_output, sender_worker);
    }));

    program_data_rc.borrow_mut().data_receiver = Some(data::DataReceiver{ server_address, worker, stream });
}

/// Returns server address if receiver was running.
pub fn stop(program_data_rc: &Rc<RefCell<ProgramData>>) -> Option<String> {
    let mut pd = program_data_rc.borrow_mut();
    if let Some(data_receiver) = &mut pd.data_receiver {
        data_receiver.stream.shutdown(std::net::Shutdown::Both).unwrap();
        data_receiver.worker.take().unwrap().join().unwrap();
        let addr = data_receiver.server_address.clone();
        pd.data_receiver = None;
        Some(addr)
    } else {
        None
    }
}

//
// Plane Tracker
// Copyright (c) 2023 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::{Deg, Point2, Rad};
use crate::{config, gui};
use std::collections::HashMap;

/// Arithmetic mean radius (R1) as per IUGG.
pub const EARTH_RADIUS: f64 = 6_371_008.8;

#[derive(Debug)]
pub struct LatLon {
    pub lat: Deg<f64>,
    pub lon: Deg<f64>
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModeSTransponderCode(u32); // value <= 0x00FFFFFF

#[derive(Debug)]
pub struct Knots(pub f64);

#[derive(Debug)]
pub struct Meters(pub f64);

#[derive(Debug)]
pub struct Feet(pub f64);

impl std::str::FromStr for ModeSTransponderCode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 6 {
            Err(format!("invalid input length ({})", s.len()).into())
        } else if s.chars().find(
            |c| !['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F'].contains(c)
        ).is_some() {
            Err(format!("input contains invalid character(s)"))
        } else {
            match u32::from_str_radix(s, 16) {
                Ok(value) => Ok(ModeSTransponderCode(value)),
                Err(e) => Err(format!("{}", e))
            }
        }
    }
}

#[derive(Debug)]
pub struct EsIdentificationAndCategory {
    pub id: ModeSTransponderCode,
    pub callsign: String
}

#[derive(Debug)]
pub struct EsAirbornePosition {
    pub id: ModeSTransponderCode,
    pub altitude: Feet,
    pub lat_lon: LatLon
}

#[derive(Debug)]
pub struct EsAirborneVelocity {
    pub id: ModeSTransponderCode,
    pub ground_speed: Knots,
    pub track: Deg<f64>
}

#[derive(Debug)]
pub struct SurveillanceAltitude {
    pub id: ModeSTransponderCode,
    pub altitude: Feet
}

#[derive(Debug)]
pub enum Sbs1Message {
    EsIdentificationAndCategory(EsIdentificationAndCategory),
    EsAirbornePosition(EsAirbornePosition),
    EsAirborneVelocity(EsAirborneVelocity),
    SurveillanceAltitude(SurveillanceAltitude)
}

impl Sbs1Message {
    pub fn id(&self) -> ModeSTransponderCode {
        match self {
            Sbs1Message::EsIdentificationAndCategory(msg) => msg.id,
            Sbs1Message::EsAirbornePosition(msg) => msg.id,
            Sbs1Message::EsAirborneVelocity(msg) => msg.id,
            Sbs1Message::SurveillanceAltitude(msg) => msg.id,
        }
    }
}

pub struct Aircraft {
    pub id: ModeSTransponderCode,
    pub callsign: Option<String>,
    pub lat_lon: Option<LatLon>,
    pub track: Option<Deg<f64>>,
    pub altitude: Option<Feet>,
    pub ground_speed: Option<Knots>,
    pub t_last_update: std::time::Instant,
}

pub struct ProgramData {
    pub observer_lat_lon: LatLon,
    aircraft: HashMap<ModeSTransponderCode, Aircraft>,
    pub gui: Option<gui::GuiData>, // always set once GUI is initialized,
    pub config: config::Configuration
}

impl ProgramData {
    pub fn new() -> ProgramData {
        let config = config::Configuration::new();

        ProgramData{
            observer_lat_lon: config.observer_lat_lon().unwrap_or(LatLon{ lat: Deg(0.0), lon: Deg(0.0) }),
            aircraft: HashMap::new(),
            gui: None,
            config
        }
    }

    pub fn aircraft(&self) -> &HashMap<ModeSTransponderCode, Aircraft> {
        &self.aircraft
    }

    pub fn update(&mut self, msg: Sbs1Message) {
        let entry = self.aircraft.entry(msg.id()).or_insert(Aircraft{
                id: msg.id(),
                callsign: None,
                lat_lon: None,
                altitude: None,
                track: None,
                ground_speed: None,
                t_last_update: std::time::Instant::now()
        });

        match msg {
            Sbs1Message::EsIdentificationAndCategory(msg) => {
                entry.callsign = Some(msg.callsign);
            },

            Sbs1Message::EsAirbornePosition(msg) => {
                entry.altitude = Some(msg.altitude);
                entry.lat_lon = Some(msg.lat_lon);
            },

            Sbs1Message::EsAirborneVelocity(msg) => {
                entry.ground_speed = Some(msg.ground_speed);
                entry.track = Some(msg.track);
            },

            Sbs1Message::SurveillanceAltitude(msg) => {
                entry.altitude = Some(msg.altitude);
            }
        }

        entry.t_last_update = std::time::Instant::now();
    }
}

pub fn project(observer: &LatLon, lat_lon: &LatLon) -> Point2<f64> {
    let rel_lat = lat_lon.lat - observer.lat;
    let rel_lon = lat_lon.lon - observer.lon;

    let x = EARTH_RADIUS * Rad::from(rel_lon).0.sin() * Rad::from(rel_lat).0.cos();
    let y = EARTH_RADIUS * Rad::from(rel_lat).0.sin();

    Point2::new(x, y)
}

//
// Plane Tracker
// Copyright (c) 2023 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::{Deg, Point2, Point3, Rad};
use crate::{config, gui};
use std::collections::HashMap;
use uom::{si::f64, si::{length, velocity}};

/// Arithmetic mean radius (R1) as per IUGG.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8; // TODO: convert to const `length::meter` once supported

#[derive(Clone, Debug)]
pub struct LatLon {
    pub lat: Deg<f64>,
    pub lon: Deg<f64>
}

pub struct GeoPos {
    pub lat_lon: LatLon,
    pub elevation: f64::Length
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModeSTransponderCode(u32); // value <= 0x00FFFFFF

fn meters(value: f64) -> f64::Length {
    f64::Length::new::<length::meter>(value)
}


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
    pub altitude: Option<f64::Length>,
    pub lat_lon: Option<LatLon>
}

#[derive(Debug)]
pub struct EsAirborneVelocity {
    pub id: ModeSTransponderCode,
    pub ground_speed: f64::Velocity,
    pub track: Deg<f64>
}

#[derive(Debug)]
pub struct SurveillanceAltitude {
    pub id: ModeSTransponderCode,
    pub altitude: f64::Length
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
    pub estimated_lat_lon: Option<LatLon>,
    pub track: Option<Deg<f64>>,
    pub altitude: Option<f64::Length>,
    pub ground_speed: Option<f64::Velocity>,
    pub t_last_update: std::time::Instant,
}

pub struct ProgramData {
    pub observer_location: GeoPos,
    aircraft: HashMap<ModeSTransponderCode, Aircraft>,
    pub gui: Option<gui::GuiData>, // always set once GUI is initialized,
    pub config: config::Configuration
}

impl ProgramData {
    pub fn new() -> ProgramData {
        let config = config::Configuration::new();

        ProgramData{
            observer_location: config.observer_location().unwrap_or(
                GeoPos{
                    lat_lon: LatLon{ lat: Deg(0.0), lon: Deg(0.0) },
                    elevation: f64::Length::new::<length::meter>(0.0)
                }
            ),
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
                estimated_lat_lon: None,
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
                entry.altitude = msg.altitude;
                entry.lat_lon = msg.lat_lon;
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

/// Returns value in meters.
pub fn project(observer: &LatLon, lat_lon: &LatLon) -> Point2<f64> {
    let rel_lat = lat_lon.lat - observer.lat;
    let rel_lon = lat_lon.lon - observer.lon;

    let x = EARTH_RADIUS_M * Rad::from(rel_lon).0.sin() * Rad::from(rel_lat).0.cos();
    let y = EARTH_RADIUS_M * Rad::from(rel_lat).0.sin();

    Point2::new(x, y)
}

/// Coordinates (meters) in Cartesian frame with lat. 0°, lon. 0°, elevation 0 being (1, 0, 0)
/// and the North Pole at (0, 0, 1).
pub fn to_global(position: &GeoPos) -> Point3<f64> {
    let (lat, lon) = (position.lat_lon.lat, position.lat_lon.lon);
    let r = EARTH_RADIUS_M + position.elevation.get::<length::meter>();
    Point3{
        x: r * Rad::from(lon).0.cos() * Rad::from(lat).0.cos(),
        y: r * Rad::from(lon).0.sin() * Rad::from(lat).0.cos(),
        z: r * Rad::from(lat).0.sin()
    }
}

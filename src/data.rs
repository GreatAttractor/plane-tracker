//
// Plane Tracker
// Copyright (c) 2023 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::{Basis3, Deg, EuclideanSpace, InnerSpace, Point2, Point3, Rotation, Rotation3, Vector3, Rad};
use crate::{config, gui};
use std::collections::HashMap;
use uom::{si::f64, si::{length, velocity}};

/// Arithmetic mean radius (R1) as per IUGG.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8; // TODO: convert to const `length::meter` once supported

const GC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
const MAX_DURATION_WITHOUT_UPDATE: std::time::Duration = std::time::Duration::from_secs(60);
const NORTH_POLE: Vector3<f64> = Vector3{ x: 0.0, y: 0.0, z: 1.0 };

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

#[derive(PartialEq)]
pub enum State {
    Normal,
    Selected
}

pub struct Aircraft {
    pub id: ModeSTransponderCode,
    pub state: State,
    pub callsign: Option<String>,
    pub lat_lon: Option<(LatLon, std::time::Instant)>, // contains time of last update
    pub estimated_lat_lon: Option<(LatLon, std::time::Instant)>, // contains time of last estimation
    pub track: Option<Deg<f64>>,
    pub altitude: Option<f64::Length>,
    pub ground_speed: Option<f64::Velocity>,
    pub t_last_update: std::time::Instant, // time of last update of any field
}

impl Aircraft {
    pub fn update_interpolated_position(&mut self, now: std::time::Instant) {
        match &self.estimated_lat_lon {
            None => {
                match (&self.lat_lon, self.track, self.ground_speed) {
                    (Some((lat_lon, t_last)), Some(track), Some(ground_speed)) => {
                        self.estimated_lat_lon = Some((estimate_position(lat_lon, track, ground_speed, now - *t_last), now));
                    },
                    _ => ()
                }
            },

            Some((est_lat_lon, t_last)) => {
                self.estimated_lat_lon =
                    Some((estimate_position(est_lat_lon, self.track.unwrap(), self.ground_speed.unwrap(), now - *t_last), now));
            },
        }
    }

    pub fn estimated_lat_lon(&self) -> Option<&LatLon> {
        self.estimated_lat_lon.as_ref().map(|ell| &ell.0)
    }
}

pub struct DataReceiver {
    pub server_address: String,
    pub worker: Option<std::thread::JoinHandle<()>>, // always `Some`
    pub stream: std::net::TcpStream // stream providing SBS1 messages
}

pub struct ProgramData {
    pub observer_location: GeoPos,
    pub aircraft: HashMap<ModeSTransponderCode, Aircraft>,
    pub gui: Option<gui::GuiData>, // always set once GUI is initialized,
    pub config: config::Configuration,
    /// Last garbage collection of `aircraft`.
    t_last_gc: std::time::Instant,
    pub data_receiver: Option<DataReceiver>,
    pub recording: bool
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
            config,
            t_last_gc: std::time::Instant::now(),
            data_receiver: None,
            recording: false
        }
    }

    pub fn update(&mut self, msg: Sbs1Message) {
        let entry = self.aircraft.entry(msg.id()).or_insert(Aircraft{
                id: msg.id(),
                state: State::Normal,
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
                if let Some(lat_lon) = msg.lat_lon {
                    if self.config.filter_ooo_messages().unwrap_or(true)
                        && entry.lat_lon.is_some()
                        && entry.track.is_some()
                        && entry.altitude.is_some()
                        && aircraft_moved_backwards(entry, &lat_lon) {

                        return;
                    }

                    entry.lat_lon = Some((lat_lon, std::time::Instant::now()));
                    if entry.estimated_lat_lon.is_some() {
                        entry.estimated_lat_lon = entry.lat_lon.clone();
                    }
                }

                entry.altitude = msg.altitude;
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

        let num_displayed_aircraft = self.aircraft
            .iter()
            .filter(|(_, aircraft)| { aircraft.lat_lon.is_some() && aircraft.track.is_some() })
            .count();
        self.gui.as_ref().unwrap().status_bar_fields.num_aircraft.set_text(&format!("Aircraft: {}", num_displayed_aircraft));
    }

    pub fn garbage_collect(&mut self) {
        if self.t_last_gc.elapsed() < GC_INTERVAL { return; }
        self.aircraft.retain(|_, aircraft| { aircraft.t_last_update.elapsed() <= MAX_DURATION_WITHOUT_UPDATE });
        self.t_last_gc = std::time::Instant::now();
    }
}

/// Orthographic projection with observer at (0, 0); value in meters.
pub fn project(observer: &LatLon, lat_lon: &LatLon) -> Point2<f64> {
    const NS: Vector3<f64> = Vector3{ x: 0.0, y: 0.0, z: 1.0 };
    const EW: Vector3<f64> = Vector3{ x: 0.0, y: 1.0, z: 0.0 };

    let rot_ns = Basis3::from_axis_angle(NS, -observer.lon);
    let rot_ew = Basis3::from_axis_angle(EW, observer.lat);

    let p = EARTH_RADIUS_M * to_xyz_unit(lat_lon).to_vec();
    let q = rot_ew.rotate_vector(rot_ns.rotate_vector(p));

    Point2{ x: q.y, y: q.z }
}

/// Orthographic projection of a distance measured along the Earth's surface (at elevation 0).
pub fn project_distance_on_earth(radius: f64::Length) -> f64::Length {
    meters(EARTH_RADIUS_M * (radius.get::<length::meter>() / EARTH_RADIUS_M).sin())
}

/// Coordinates (meters) in Cartesian frame with lat. 0°, lon. 0°, elevation 0 being (1, 0, 0)
/// and the North Pole at (0, 0, 1).
fn to_xyz_unit(lat_lon: &LatLon) -> Point3<f64> {
    let (lat, lon) = (lat_lon.lat, lat_lon.lon);
    Point3{
        x: Rad::from(lon).0.cos() * Rad::from(lat).0.cos(),
        y: Rad::from(lon).0.sin() * Rad::from(lat).0.cos(),
        z: Rad::from(lat).0.sin()
    }
}

pub fn to_global(position: &GeoPos) -> Point3<f64> {
    let r = EARTH_RADIUS_M + position.elevation.get::<length::meter>();
    r * to_xyz_unit(&position.lat_lon)
}

fn meters(value: f64) -> f64::Length {
    f64::Length::new::<length::meter>(value)
}

fn estimate_position(
    start: &LatLon,
    track: Deg<f64>,
    ground_speed: f64::Velocity,
    duration: std::time::Duration
) -> LatLon {
    let pos = to_xyz_unit(start);

    let r = pos.to_vec();
    let n_pole = Point3{ x: 0.0, y: 0.0, z: 1.0 };
    let p = (n_pole - pos).normalize();
    let track_rot = Basis3::from_axis_angle(r, -track);
    let q = track_rot.rotate_vector(p);
    let forward_angle = Rad(ground_speed.get::<velocity::meter_per_second>() * duration.as_secs_f64() / EARTH_RADIUS_M);
    let forward_rot = Basis3::from_axis_angle(Vector3::cross(q, r), -forward_angle);

    let est_p = forward_rot.rotate_point(pos);

    let lat = Deg::from(Rad(est_p.z.asin()));
    let lon = Deg::from(Rad(f64::atan2(est_p.y, est_p.x)));

    LatLon{ lat, lon }
}

/// Assumes level flight; returns unit vector in global frame.
fn get_travel_dir(aircraft: &Aircraft) -> Vector3<f64> {
    let r = to_xyz_unit(&aircraft.lat_lon.as_ref().unwrap().0).to_vec();
    let s = NORTH_POLE.cross(r);
    let to_north = r.cross(s).normalize();

    let rot = Basis3::from_axis_angle(r, -aircraft.track.unwrap());

    rot.rotate_vector(to_north)
}

fn aircraft_moved_backwards(aircraft: &Aircraft, new_pos: &LatLon) -> bool {
    let old_xyz = to_xyz_unit(
        match aircraft.estimated_lat_lon.as_ref() {
            Some((lat_lon, _)) => &lat_lon,
            None => &aircraft.lat_lon.as_ref().unwrap().0
        }
    );
    let new_xyz = to_xyz_unit(new_pos);

    (new_xyz - old_xyz).dot(get_travel_dir(aircraft)) < 0.0
}

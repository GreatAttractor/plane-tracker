//
// Plane Tracker
// Copyright (c) 2023-2024 Filip Szczerek <ga.software@yahoo.com>
//
// This project is licensed under the terms of the MIT license
// (see the LICENSE file for details).
//

use cgmath::Deg;
use gtk::glib;
use gtk4 as gtk;
use pointing_utils::{GeoPos, LatLon, uom};
use std::error::Error;
use uom::{si::f64, si::length};

const DEF_DATA_SENDER_PORT: u16 = 45500;

mod groups {
    pub const UI: &str = "UI";
    pub const MAIN: &str = "Main";
}

mod keys {
    // group: MAIN
    pub const OBSERVER_LOCATION: &str = "ObserverLocation";
    pub const SERVER_ADDRRESS: &str = "ServerAddress";
    pub const FILTER_OOO_MSGS: &str = "FilterOoOMessages";
    pub const INTERPOLATE_POSITIONS: &str = "InterpolatePositions";
    pub const DATA_SENDER_PORT: &str = "DataSenderPort";

    // group: UI
    pub const MAIN_WINDOW_POS_SIZE: &str = "MainWindowPosSize";
    pub const MAIN_WINDOW_MAXIMIZED: &str = "MainWindowMaximized";
    pub const TEXT_SCALE: &str = "TextScale";
}

pub struct Configuration {
    key_file: glib::KeyFile
}

impl Configuration {
    pub fn store(&self) -> Result<(), glib::error::Error> {
        self.key_file.save_to_file(config_file_path())
    }

    pub fn new() -> Configuration {
        let key_file = glib::KeyFile::new();
        let file_path = config_file_path();
        if key_file.load_from_file(
            file_path.clone(),
            glib::KeyFileFlags::NONE
        ).is_err() {
            println!("WARNING: Failed to load configuration from {}.", file_path.to_str().unwrap());
        }

        Configuration{ key_file }
    }

    pub fn text_scale(&self) -> Result<f64, Box<dyn Error>> {
        let value = self.key_file.double(groups::UI, keys::TEXT_SCALE)?;
        if value > 0.0 {
            Ok(value)
        } else {
            Err(format!("{}:{} must be positive", groups::UI, keys::TEXT_SCALE).into())
        }
    }

    pub fn set_text_scale(&self, value: f64) {
        assert!(value > 0.0);
        self.key_file.set_double(groups::UI, keys::TEXT_SCALE, value);
    }

    pub fn filter_ooo_messages(&self) -> Result<bool, Box<dyn Error>> {
        Ok(self.key_file.boolean(groups::MAIN, keys::FILTER_OOO_MSGS)?)
    }

    pub fn set_filter_ooo_messages(&self, value: bool) {
        self.key_file.set_boolean(groups::MAIN, keys::FILTER_OOO_MSGS, value);
    }

    pub fn server_address(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.key_file.string(groups::MAIN, keys::SERVER_ADDRRESS)?.as_str().into())
    }

    pub fn set_server_address(&self, server_address: &str) {
        self.key_file.set_string(groups::MAIN, keys::SERVER_ADDRRESS, server_address);
    }

    pub fn observer_location(&self) -> Result<GeoPos, Box<dyn Error>> {
        let ll_str = self.key_file.string(groups::MAIN, keys::OBSERVER_LOCATION)?;
        let values: Vec<&str> = ll_str.split(';').collect();
        if values.len() != 3 { return Err("too few values".into()); }
        Ok(GeoPos{
            lat_lon: LatLon{
                lat: Deg(values[0].parse::<f64>()?),
                lon: Deg(values[1].parse::<f64>()?)
            },
            elevation: f64::Length::new::<length::meter>(values[2].parse::<f64>()?)
        })
    }

    pub fn main_window_pos(&self) -> Option<gtk::gdk::Rectangle> {
        self.read_rect(groups::UI, keys::MAIN_WINDOW_POS_SIZE)
    }

    pub fn set_main_window_pos(&self, pos_size: gtk::gdk::Rectangle) {
        self.store_rect(groups::UI, keys::MAIN_WINDOW_POS_SIZE, pos_size);
    }

    pub fn main_window_maximized(&self) -> Option<bool> {
        self.key_file.boolean(groups::UI, keys::MAIN_WINDOW_MAXIMIZED).ok()
    }

    pub fn set_main_window_maximized(&self, value: bool) {
        self.key_file.set_boolean(groups::UI, keys::MAIN_WINDOW_MAXIMIZED, value);
    }

    fn store_rect(&self, group: &str, key: &str, rect: gtk::gdk::Rectangle) {
        self.key_file.set_string(group, key, &format!("{};{};{};{}", rect.x(), rect.y(), rect.width(), rect.height()));
    }

    fn read_rect(&self, group: &str, key: &str) -> Option<gtk::gdk::Rectangle> {
        let rect_str = match self.key_file.string(group, key) {
            Ok(s) => s,
            Err(_) => return None
        };

        let mut numbers: Vec<i32> = vec![];
        for frag in rect_str.split(';') {
            let num = match frag.parse::<i32>() {
                Ok(n) => n,
                Err(_) => {
                    println!("WARNING: invalid configuration value for {}/{}: {}", group, key, frag);
                    return None;
                }
            };
            numbers.push(num);
        }

        if numbers.len() != 4 {
            println!("WARNING: invalid configuration value for {}/{}: {}", group, key, rect_str);
            return None;
        }

        Some(gtk::gdk::Rectangle::new(numbers[0], numbers[1], numbers[2], numbers[3]))
    }

    pub fn set_interpolate_positions(&self, value: bool) {
        self.key_file.set_boolean(groups::MAIN, keys::INTERPOLATE_POSITIONS, value);
    }

    pub fn interpolate_positions(&self) -> Option<bool> {
        self.key_file.boolean(groups::MAIN, keys::INTERPOLATE_POSITIONS).ok()
    }

    pub fn data_sender_port(&self) -> u16 {
        self.key_file.integer(groups::MAIN, keys::DATA_SENDER_PORT).map(|i| i as u16).unwrap_or(DEF_DATA_SENDER_PORT)
    }
}

fn config_file_path() -> std::path::PathBuf {
    std::path::Path::new(
        &dirs::config_dir().or(Some(std::path::Path::new("").to_path_buf())).unwrap()
    ).join("plane-tracker.cfg")
}

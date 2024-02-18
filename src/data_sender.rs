use crate::data::Aircraft;
use gtk4 as gtk;
use gtk::glib;
use pointing_utils::{
    GeoPos, TargetInfoMessage, to_global, to_global_velocity, to_local_point, to_local_vec, uom::si::velocity
};
use std::io::Write;

pub fn start_listener(port: u16, sender_worker: glib::Sender<std::net::TcpStream>) {
    std::thread::spawn(move || {
        let listener = std::net::TcpListener::bind(format!("localhost:{}", port)).unwrap();
        loop {
            let (stream, _) = listener.accept().unwrap();
            sender_worker.send(stream).unwrap();
        }
    });
}

pub fn send_data(aircraft: &Aircraft, observer: &GeoPos, streams: &mut [std::net::TcpStream]) {
    let aircraft_geo_pos = GeoPos{
        lat_lon: match &aircraft.estimated_lat_lon {
            Some(ell) => ell.0.clone(),
            None => aircraft.lat_lon.as_ref().unwrap().0.clone()
        },
        elevation: *aircraft.altitude.as_ref().unwrap()
    };
    let aircraft_pos = to_global(&aircraft_geo_pos);
    let observer_pos = to_global(observer);
    let position = to_local_point(&observer_pos, &aircraft_pos);
    let velocity = to_local_vec(
        &observer_pos,
        &to_global_velocity(
            &aircraft_geo_pos,
            *aircraft.track.as_ref().unwrap(),
            aircraft.ground_speed.as_ref().unwrap().get::<velocity::meter_per_second>()
        )
    );

    let message = TargetInfoMessage{
        position,
        velocity,
        track: *aircraft.track.as_ref().unwrap(),
        altitude: aircraft_geo_pos.elevation
    };

    // TODO: ignore disconnected
    for stream in streams {
        stream.write(message.to_string().as_bytes()).unwrap();
    }
}

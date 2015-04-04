extern crate time;
extern crate zoneinfo;

use zoneinfo::ZoneInfo;
use std::error::Error;

fn main() {
    let regions = ZoneInfo::get_tz_locations();

    for region in regions {
        match ZoneInfo::by_tz(&region) {
            Ok(zoneinfo) => {
                println!("{}: {}", region, zoneinfo.get_dst_specifier());
            },
            Err(error) => {
                println!("{}: unable to parse: {}", region, error.description());
            }
        }
    }
}

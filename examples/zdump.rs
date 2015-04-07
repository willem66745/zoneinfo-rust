// This example tries to emulate zdump(8) verbose output

extern crate zoneinfo;
extern crate time;

use zoneinfo::ZoneInfo;
use time::{at_utc, Timespec};
use std::env::args;

fn main() {
    let info = match args().nth(1) {
        Some(region) => ZoneInfo::by_tz(&region).unwrap(),
        None => ZoneInfo::get_local_zoneinfo().unwrap()
    };

    let all = info.get_transitions();
    let mut initial = true;
    let (_, mut old_info) = all.iter().next().unwrap();

    for (time, info) in all.iter() {
        if initial {
            /* Initial timestamp is always a historic time definition with
             * infinite negative timestamp which is cannot be printed.
             */
            initial = false;
        }
        else {
            let oldtime = Timespec::new(time.sec - 1, 0);
            let oldtime_loc = Timespec::new(oldtime.sec + old_info.ut_offset as i64, 0);
            let time_loc = Timespec::new(time.sec + info.ut_offset as i64, 0);

            println!("{} UT = {} {} isdst={} gmtoff={}",
                    at_utc(oldtime).asctime(),
                    at_utc(oldtime_loc).asctime(),
                    old_info.abbreviation,
                    if old_info.isdst {1} else {0},
                    old_info.ut_offset);
            println!("{} UT = {} {} isdst={} gmtoff={}",
                    at_utc(*time).asctime(),
                    at_utc(time_loc).asctime(),
                    info.abbreviation,
                    if info.isdst {1} else {0},
                    info.ut_offset);
        }
        old_info = info;
    }
}

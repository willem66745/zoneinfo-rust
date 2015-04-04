extern crate time;
extern crate zoneinfo;

use zoneinfo::ZoneInfo;

fn main() {
    // Yes, in some regions this example will crash, when is no such a thing as daylight saving
    // time
    let info = ZoneInfo::get_local_zoneinfo().unwrap();
    let now = time::now_utc().to_timespec();

    let actual = info.get_actual_zoneinfo(now).unwrap();

    // A very Northern/Mid-europe based example ;-)
    println!("It's {}", if actual.isdst {"Summertime!"} else {"cold :("});

    let (next, info) = info.get_next_transition_time(now).unwrap();

    println!("And it will change again at {} (to {})", time::at(next).asctime(), info.abbreviation);
}


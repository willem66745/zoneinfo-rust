//! This crate provides utilities to parse zoneinfo data. Only tested with
//! Linux, although support is expected for all flavors of *nix including
//! Darwin. Windows users might use it by downloading zoneinfo data from a
//! Linux distribution, for example
//! https://www.archlinux.org/packages/core/any/tzdata/download.

extern crate byteorder;
extern crate time;

mod visitdir;

use std::error::Error;
use std::fs::{File, metadata};
use std::path::{Path, PathBuf};
use std::io::{Read, Cursor};
use byteorder::{BigEndian, ReadBytesExt};
use time::Timespec;
use std::collections::BTreeMap;

// format is described in timezone/tzfile.h of the GNU libc library
#[derive(Debug)]
struct TzHeadInner {
    tzh_magic: String, // TZ_MAGIC
    tzh_version: char, // '\0' or '2' or '3' as of 2013
    // 15 bytes (reserved; must be zero)
    tzh_ttigmtcnt: u32, // coded number of trans. time flags
    tzh_ttisstdcnt: u32, // coded number of trans. time flags
    tzh_leapcnt: u32, // coded number of leap seconds
    tzh_timecnt: u32, // coded number of transition times
    tzh_typecnt: u32, // coded number of local time types
    tzh_charcnt: u32, // coded number of abbr. chars
}

struct TzHead<F: Fn(&mut Cursor<&[u8]>)->Result<i64, byteorder::Error>> {
    inner: TzHeadInner,
    time_consumer: F
}

#[derive(Debug)]
struct TzType {
    ut_offset: i32,
    isdst: bool,
    abbreviation: String
}

#[derive(Debug, Copy, Clone)]
/// Attributes associated to transition
pub enum TransitionTimeFlag {
    Standard,
    WallClock,
    Universal,
    Local
}

impl <F: Fn(&mut Cursor<&[u8]>)->Result<i64, byteorder::Error>>TzHead<F> {
    /// returns parsed zoneinfo header
    fn new(reader: &mut Cursor<&[u8]>, x: F) -> Result<TzHead<F>, byteorder::Error> {
        let mut magic:[u8; 4] = [0;4];
        try!(reader.read(&mut magic));
        let version = try!(reader.read_u8());
        let position = reader.position();
        reader.set_position(position + 15); // skip reserved bytes
        let ttigmtcnt = try!(reader.read_u32::<BigEndian>());
        let ttisstdcnt = try!(reader.read_u32::<BigEndian>());
        let leapcnt = try!(reader.read_u32::<BigEndian>());
        let timecnt = try!(reader.read_u32::<BigEndian>());
        let typecnt = try!(reader.read_u32::<BigEndian>());
        let charcnt = try!(reader.read_u32::<BigEndian>());

        Ok(TzHead {
            inner: TzHeadInner {
                tzh_magic: std::str::from_utf8(&magic).unwrap().to_string(), // FIXME: remove unwrap
                tzh_version: version as char,
                tzh_ttigmtcnt: ttigmtcnt,
                tzh_ttisstdcnt: ttisstdcnt,
                tzh_leapcnt: leapcnt,
                tzh_timecnt: timecnt,
                tzh_typecnt: typecnt,
                tzh_charcnt: charcnt,
            },
            time_consumer: x
        })
    }

    /// returns coded transition times
    ///
    /// the function assumes that the provided cursor is located at the the start of the
    /// table with transition times.
    fn decode_transition_times(&self, reader: &mut Cursor<&[u8]>) -> Result<Vec<Timespec>, byteorder::Error> {
        let mut transition_times = Vec::<Timespec>::new();

        for _ in 0..self.inner.tzh_timecnt {
            transition_times.push(Timespec::new(try!((self.time_consumer)(reader)), 0));
        }

        Ok(transition_times)
    }

    /// returns types of local time startings
    ///
    /// the function assumes that the provided cursor is located at the the start of the
    /// table with transition types.
    fn decode_transition_types(&self, reader: &mut Cursor<&[u8]>) -> Result<Vec<u8>, byteorder::Error> {
        let mut transition_types = Vec::<u8>::new();

        for _ in 0..self.inner.tzh_timecnt {
            transition_types.push(try!(reader.read_u8()));
        }

        Ok(transition_types)
    }

    /// returns local time startings data
    ///
    /// this function consumes the char buffer as well to incorperate it in the result
    ///
    /// the function assumes that the provided cursor is located at the the start of the
    /// table with local time startings data
    fn decode_local_time_data(&self, reader: &mut Cursor<&[u8]>) -> Result<Vec<TzType>, byteorder::Error> {
        let mut local_time_data = Vec::<TzType>::new();
        let mut raw_local_time_data = vec![];

        for _ in 0..self.inner.tzh_typecnt {
            let ut_offset = try!(reader.read_i32::<BigEndian>());
            let isdst = try!(reader.read_u8());
            let abbr_index = try!(reader.read_u8());

            raw_local_time_data.push((ut_offset, isdst, abbr_index));
        }

        let mut charbuf = vec![0u8; self.inner.tzh_charcnt as usize];
        try!(reader.read(&mut charbuf[..]));

        for (ut_offset, isdst, abbr_index) in raw_local_time_data {
            // In C: strcpy(abbreviation, &charbuf[abbr_index]) -- also a solution possible without clone?
            let abbr:Vec<_> = charbuf.clone().into_iter()
                                     .skip(abbr_index as usize)
                                     .take_while(|&c| c > 0)
                                     .collect();
            let abbreviation = std::str::from_utf8(&abbr[..]).unwrap(); // FIXME: improve error handling
            local_time_data.push(TzType{
                ut_offset: ut_offset,
                isdst: isdst != 0,
                abbreviation: abbreviation.to_string(),
            })
        }

        Ok(local_time_data)
    }

    /// returns a list of leap seconds transition changes
    ///
    /// the function assumes that the provided cursor is located at the the start of the
    /// table with leap second transitions
    fn decode_leap_second_corrections(&self, reader: &mut Cursor<&[u8]>) -> Result< Vec<(Timespec, i32)>, byteorder::Error> {
        let mut leap_second_corrections = vec![];

        for _ in 0..self.inner.tzh_leapcnt {
            let transition_time = try!((self.time_consumer)(reader));
            let seconds = try!(reader.read_i32::<BigEndian>());

            leap_second_corrections.push((Timespec::new(transition_time as i64, 0),
                                          seconds));
        }

        Ok(leap_second_corrections)
    }

    /// returns a list of transition moment flags (wall clock or standard)
    ///
    /// the function assumes that the provided cursor is located at the the start of the
    /// table with wall clock or standard transition moments
    fn decode_transition_flags1(&self, reader: &mut Cursor<&[u8]>) -> Result< Vec<TransitionTimeFlag>, byteorder::Error> {
        let mut transition_flags = vec![];

        for _ in 0..self.inner.tzh_ttisstdcnt {
            transition_flags.push(match try!(reader.read_u8()) {
                0 => TransitionTimeFlag::WallClock,
                _ => TransitionTimeFlag::Standard,
            })
        }

        Ok(transition_flags)
    }

    /// returns a list of transition moment flags (local or universal)
    ///
    /// the function assumes that the provided cursor is located at the the start of the
    /// table with local or universal transition moments
    fn decode_transition_flags2(&self, reader: &mut Cursor<&[u8]>) -> Result< Vec<TransitionTimeFlag>, byteorder::Error> {
        let mut transition_flags = vec![];

        for _ in 0..self.inner.tzh_ttigmtcnt {
            transition_flags.push(match try!(reader.read_u8()) {
                0 => TransitionTimeFlag::Local,
                _ => TransitionTimeFlag::Universal,
            })
        }

        Ok(transition_flags)
    }
}

struct ZoneInfoInner {
    header: TzHeadInner,
    transision_times: Vec<Timespec>,
    transision_types: Vec<u8>,
    local_times: Vec<TzType>,
    leap_seconds_data: Vec<(Timespec, i32)>,
    transition_flags1: Vec<TransitionTimeFlag>,
    transition_flags2: Vec<TransitionTimeFlag>
}

fn read_zone_info<F: Fn(&mut Cursor<&[u8]>)->Result<i64, byteorder::Error>>
            (cursor: &mut Cursor<&[u8]>, x: F) -> Result<ZoneInfoInner, std::io::Error> {
    let header = try!(TzHead::new(cursor, x));
    let mut transition_times = try!(header.decode_transition_times(cursor));
    let mut transition_types = try!(header.decode_transition_types(cursor));
    let local_times = try!(header.decode_local_time_data(cursor));
    let leap_seconds_data = try!(header.decode_leap_second_corrections(cursor));
    let transition_flags1 = try!(header.decode_transition_flags1(cursor));
    let transition_flags2 = try!(header.decode_transition_flags2(cursor));

    // when only a single time definition exists and no single transition create a dummy
    // transition. This to support zoneinfo files which are part of the Debian, Ubuntu, Mint
    // distribution family.
    if transition_times.len() == 0 && local_times.len() == 1 {
        transition_times.push(Timespec::new(std::i64::MIN, 0));
        transition_types.push(0);
    }

    Ok(ZoneInfoInner {
        header: header.inner,
        transision_times: transition_times,
        transision_types: transition_types,
        local_times: local_times,
        leap_seconds_data: leap_seconds_data,
        transition_flags1: transition_flags1,
        transition_flags2: transition_flags2
    })
}

fn consume_32bit_timestamps(reader: &mut Cursor<&[u8]>) -> Result<i64, byteorder::Error> {
    Ok(try!(reader.read_i32::<BigEndian>()) as i64)
}
fn consume_64bit_timestamps(reader: &mut Cursor<&[u8]>) -> Result<i64, byteorder::Error> {
    reader.read_i64::<BigEndian>()
}

/// Transition details
#[derive(Debug, Clone)]
pub struct ZoneInfoElement {
    /// Offset to UTC in seconds
    pub ut_offset: i32,
    /// Daylight saving time
    pub isdst: bool,
    /// Ebbreviation of the time zone
    pub abbreviation: String,
    /// Transition time wall-time or governmental controlled
    pub wall_clock_or_standard: TransitionTimeFlag,
    /// Transition time is local time or UTC
    pub local_or_universal_time: TransitionTimeFlag,
}

/// Time zone information
pub struct ZoneInfo {
    zone_info:ZoneInfoInner,
    time_zone_specifier:String
}

impl ZoneInfo {
    /// Load zone info from a provided `tzfile(5)`. These files are often
    /// located in `/usr/share/zoneinfo` or `/usr/local/share/info`. Depending on
    /// your system the systems zoneinfo file is located in `/etc/localtime`.
    pub fn new(zoneinfofile: &Path) -> Result<ZoneInfo, std::io::Error> {
        let mut file = try!(File::open(&zoneinfofile));
        let mut buffer = Vec::<u8>::new();
        try!(file.read_to_end(&mut buffer));
        let mut cursor = Cursor::new(&buffer[..]);
        let mut tail = String::new();

        let tz:ZoneInfoInner;
        let b32 = try!(read_zone_info(&mut cursor, consume_32bit_timestamps));
        if b32.header.tzh_version == '2' ||
           b32.header.tzh_version == '3' {
            let b64 = try!(read_zone_info(&mut cursor, consume_64bit_timestamps));
            // during testing 64 bit variants can't be used on 32-bit systems
            // due to different glibc2 behavior (which is used as backend format
            // for Linux systems)
            if cfg!(target_pointer_width = "64") {
                tz = b64;
            }
            else
            {
                tz = b32;
            }
            cursor.read_to_string(&mut tail).unwrap();
        }
        else {
           tz = b32;
        }

        Ok(ZoneInfo{zone_info:tz, time_zone_specifier:tail})
    }

    /// Load zone info based on a provided location.
    ///
    /// ```rust
    /// use zoneinfo::ZoneInfo;
    /// let info = ZoneInfo::by_tz("Europe/Amsterdam").unwrap();
    ///
    /// println!("Daylight saving time rules are: {}", info.get_dst_specifier());
    /// ```
    ///
    /// Not available for Windows users
    pub fn by_tz(location: &str) -> Result<ZoneInfo, std::io::Error> {
        let all = ZoneInfo::get_tz_locations();
        if !all.contains(&location.to_string()) {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound,
                "provided location not found"));
        }

        let zoneinfo;
        let mut try_location = PathBuf::from("/usr/share/zoneinfo");
        try_location.push(location);

        // this could have be very simple whether try_location.is_file()
        // would have be stable.
        let meta = metadata(&try_location);
        let try_alternative = match meta {
            Ok(m) => !m.is_file(),
            Err(_) => true
        };

        if try_alternative {
            let mut try_location = PathBuf::from("/usr/local/share/zoneinfo");
            try_location.push(location);
            zoneinfo = try_location;
        }
        else {
            zoneinfo = try_location;
        }

        ZoneInfo::new(&zoneinfo)
    }

    /// Retrieve local zoneinfo settings
    ///
    /// Not available for Windows users
    pub fn get_local_zoneinfo() -> Result<ZoneInfo, std::io::Error> {
        ZoneInfo::new(&Path::new("/etc/localtime"))
    }

    /// Retrieve all supported zoneinfo locations available at this machine.
    /// These locations can be used by `ZoneInfo::by_tz`.
    ///
    /// Not available for Windows users
    pub fn get_tz_locations() -> Vec<String> {
        let mut zones = vec![];

        let used_zoneinfo;
        let zoneinfo = Path::new("/usr/share/zoneinfo");

        let _ = visitdir::visit_dirs(zoneinfo, &mut {|x| zones.push(x)});

        if zones.len() == 0 {
            let zoneinfo = Path::new("/usr/local/share/zoneinfo");
            let _ = visitdir::visit_dirs(zoneinfo, &mut {|x| zones.push(x)});
            used_zoneinfo = zoneinfo;
        }
        else
        {
            used_zoneinfo = zoneinfo;
        }

        let skip = used_zoneinfo.components().count();

        let mut items = vec![];

        for zone in zones {
            let path = zone.path();
            let without_parent = path.components().skip(skip);
            let mut rel_path = PathBuf::new();

            for part in without_parent {
                rel_path.push(part.as_os_str());
            }

            if let Some(n) = rel_path.to_str() {
                items.push(n.to_string());
            }
        }

        items.sort();

        items
    }

    /// Get all transitions as a map of transition timestamps (`time::Timespec`)
    /// and information associated to that transition (offset from UTC,
    /// (timezone) abbreviation, and a daylight saving time indication).
    ///
    /// Please note that the initial timestamp is `std::i64::MIN` (when using
    /// a 64-bit OS) and cannot be printed as timestamp.
    pub fn get_transitions(&self) -> BTreeMap<Timespec, ZoneInfoElement> {
        let mut map = BTreeMap::<Timespec, ZoneInfoElement>::new();

        for (time, type_index) in self.zone_info
                                      .transision_times
                                      .iter()
                                      .zip(self.zone_info.transision_types.iter()) {
            let info = &self.zone_info.local_times[*type_index as usize];
            let el = ZoneInfoElement {
                ut_offset: info.ut_offset,
                isdst: info.isdst,
                abbreviation: info.abbreviation.clone(),
                wall_clock_or_standard: self.zone_info.transition_flags1[*type_index as usize],
                local_or_universal_time: self.zone_info.transition_flags2[*type_index as usize],
            };
            let _ = map.insert(time.clone(), el);
        }

        map
    }

    /// Get all leap second transitions which are coded in the zoneinfo file as
    /// a map of timestamps and offset towards to previous time.
    pub fn get_leap_second_transitions(&self) -> BTreeMap<Timespec, i32> {
        let mut map = BTreeMap::<Timespec, i32>::new();

        for &(time, duration) in self.zone_info.leap_seconds_data.iter() {
            map.insert(time.clone(), duration.clone());
        }

        map
    }

    /// Return zone info relevant for the provided timestamp
    ///
    /// ```rust
    /// extern crate time;
    /// extern crate zoneinfo;
    ///
    /// use zoneinfo::ZoneInfo;
    ///
    /// fn main() {
    ///     let info = ZoneInfo::get_local_zoneinfo().unwrap();
    ///
    ///     let actual = info.get_actual_zoneinfo(time::now_utc().to_timespec()).unwrap();
    ///
    ///     // A very Northern/Mid-europe based example ;-)
    ///     println!("It's {}", if actual.isdst {"Summertime!"} else {"cold :("});
    /// }
    /// ```
    pub fn get_actual_zoneinfo(&self, timestamp: Timespec) -> Option<ZoneInfoElement> {
        let transitions = self.get_transitions();

        if let Some((_, zoneinfo)) = transitions.iter()
                                                .take_while(|&(x,_)| *x < timestamp)
                                                .last() {
            Some(zoneinfo.clone())
        }
        else {
            None
        }
    }

    /// Returns as a tuple a timestamp and related information when the next transaction will take
    /// place.
    ///
    /// Note that in some regions there is no DST, and this function will return None.
    pub fn get_next_transition_time(&self, timestamp: Timespec) -> Option<(Timespec, ZoneInfoElement)> {
        let transitions = self.get_transitions();

        if let Some((time, zoneinfo)) = transitions.iter()
                                                .skip_while(|&(x,_)| *x < timestamp)
                                                .next() {
            Some((*time, zoneinfo.clone()))
        }
        else {
            None
        }
    }

    /// Retrieve the daylight saving time rules for loaded zoneinfo.
    pub fn get_dst_specifier(&self)->String {
        self.time_zone_specifier.trim().to_string()
    }
}

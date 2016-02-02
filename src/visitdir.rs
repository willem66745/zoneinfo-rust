// Since fs::walk_dir is unstable, a own implementation; hopefully a later
// release will include a variant of it and this module can be removed.
use std::io;
use std::fs::{self, DirEntry, metadata};
use std::path::Path;

// one possible implementation of fs::walk_dir only visiting files
pub fn visit_dirs(dir: &Path, cb: &mut FnMut(DirEntry)) -> io::Result<()> {
    let meta = try!(metadata(dir));
    // if dir.is_dir() {
    if meta.is_dir() {
        for entry in try!(fs::read_dir(dir)) {
            let entry = try!(entry);
            let meta = try!(metadata(entry.path()));
            //if entry.path().is_dir() {
            if meta.is_dir() {
                try!(visit_dirs(&entry.path(), cb));
            } else {
                cb(entry);
            }
        }
    }
    Ok(())
}

use crate::ls::parse_long_list;
use crate::{cmd::SshCmd, ls::FileMeta};
use fuse_mt::*;
use libc;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::SystemTime;
use std::{collections::HashMap, path::PathBuf};
use std::{ffi::OsStr, time::Instant};

const TTL: Duration = Duration::from_secs(60);

/// helper to mount a path
pub fn mount(runner: SshCmd) {
    let fuse_args: Vec<&OsStr> = vec![
        &OsStr::new("-o"),
        &OsStr::new("auto_unmount"),
        &OsStr::new("ro"),
    ];

    let mount_point = format!("/tmp/test");

    let path = Path::new(&mount_point);
    if !path.exists() {
        let _ = fs::create_dir(path);
    }

    // unmount target if it exiss
    let _ = Command::new("umount").arg(path).spawn().map_err(|e| {
        println!("umount {:?}", e);
    });

    let filesystem = SshFuseFs::new(runner);

    fuse_mt::mount(
        fuse_mt::FuseMT::new(filesystem, 10),
        &mount_point,
        &fuse_args,
    )
    .unwrap();
}

#[derive(Debug)]
struct CachedMeta {
    file_meta: Option<FileMeta>,
    directory: bool,
    perms: u16,
    size: u64,
    children: Option<Vec<String>>,
    updated: bool,
    last_updated: Instant,
}

/// this is a file system back by a cache built on the fly from a remote
/// listing. the list command is currently on done on the parent and hence
/// would not have complete data. Ideally this could be merged from stat
/// information
struct SshFuseFs {
    runner: SshCmd,
    cache: Arc<Mutex<HashMap<String, CachedMeta>>>,
}

impl SshFuseFs {
    fn new(runner: SshCmd) -> Self {
        SshFuseFs {
            runner,
            cache: Default::default(),
        }
    }

    fn check_and_update(&self, path_str: &str) {
        let mut buf = PathBuf::from(path_str);
        buf.pop();
        let parent_path = buf.to_str().unwrap();

        let in_cache = {
            let cache = self.cache.lock().unwrap();

            if cache.contains_key(path_str) {
                true
            } else {
                // if parent's listing is updated, use the cache!
                match cache.get(parent_path) {
                    Some(meta) => meta.updated && meta.last_updated.elapsed() < TTL,
                    _ => false,
                }
            }
        };

        if !in_cache {
            self.update_dir_cache(parent_path, parent_path);
        }
    }

    fn fetch_path(&self, path: &str) -> Option<Vec<FileMeta>> {
        // TODO integrate the spinner views
        let cmd = format!("ls -l {}", path);
        let output = self.runner.get_output(&cmd).expect("output");

        if output.stderr.len() > 0 {
            return None;
        }
        let str = &output.stdout;
        let stdout_utf8 = str::from_utf8(&str).unwrap();
        println!("Out: {}", stdout_utf8);

        let dir = parse_long_list(stdout_utf8);

        Some(dir)
    }

    /// this uses 2 path parameters
    /// the path without slash is used to populate the cache's keys
    /// path with the forward slash is to force `ls` to list the directory
    /// content and not just the path
    fn update_dir_cache(&self, path: &str, no_trailing_key: &str) {
        let no_trailing_key = if path == "/" { "" } else { no_trailing_key };

        let meta = match self.fetch_path(path) {
            Some(meta) => meta,
            _ => return,
        };

        let mut cache = self.cache.lock();
        let cache = cache.as_mut().unwrap();

        // populate cache
        let children = meta.iter().map(|m| m.name.to_string()).collect::<Vec<_>>();

        let key = if path == "/" { "/" } else { no_trailing_key };

        // TODO do not override previous values
        // update parent info
        cache.insert(
            key.into(),
            CachedMeta {
                file_meta: None,
                directory: true,
                size: 0,
                perms: (0o7777) as u16,
                children: Some(children),
                updated: true,
                last_updated: Instant::now(),
            },
        );

        // update children
        for m in meta {
            let child_key = format!("{}/{}", no_trailing_key, m.name);
            cache.insert(
                child_key,
                CachedMeta {
                    file_meta: Some(m.clone()),
                    directory: m.directory,
                    size: m.file_size as u64,
                    perms: m.perms,
                    children: None,
                    updated: false, // this means that if it's a directory, children of this directory needs another fetch
                    last_updated: Instant::now(),
                },
            );
        }

        // println!("Cache {:#?}", cache);
    }

    /// attempts to get directory listing from catch, other make a fetch
    /// to populate cache.
    /// this is used by readdir
    fn get_dir_list_from_cache(&self, path: &str) -> Vec<DirectoryEntry> {
        let dir_path = if path.ends_with("/") {
            path.to_string()
        } else {
            format!("{}/", path)
        };

        let no_trailing_key = dir_path.get(0..dir_path.len() - 1).unwrap().to_string();

        let require_update = {
            let cache = self.cache.lock().unwrap();

            let cached = cache.get(if dir_path == "/" {
                "/"
            } else {
                &no_trailing_key
            });
            cached.is_none()
                || !cached.unwrap().updated
                || cached.unwrap().last_updated.elapsed() > TTL
        };

        if require_update {
            self.update_dir_cache(&dir_path, &no_trailing_key);
        }

        let mut entries: Vec<DirectoryEntry> = vec![];

        let cache = self.cache.lock().unwrap();

        // read from cache
        let cached = cache
            .get(if dir_path == "/" {
                "/"
            } else {
                &no_trailing_key
            })
            .unwrap();

        if let Some(children) = &cached.children {
            for filename in children {
                let name = OsString::from(filename);

                let child = cache
                    .get(&format!("{}/{}", no_trailing_key, filename))
                    .unwrap();

                let kind = if child.directory {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };

                entries.push(DirectoryEntry { name, kind })
            }
        }

        entries
    }

    fn get_entries(&self, path: &Path) -> Vec<DirectoryEntry> {
        let path = path.to_str().unwrap();

        self.get_dir_list_from_cache(path)
    }
}

impl FilesystemMT for SshFuseFs {
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        println!("init {:?}", _req);
        Ok(())
    }

    fn destroy(&self, _req: RequestInfo) {
        println!("destroy");
        // Nothing.
    }

    fn getattr(&self, _req: RequestInfo, path: &std::path::Path, _fh: Option<u64>) -> ResultEntry {
        // println!("getattr: {:?}", path);

        let path_str = path.to_str().unwrap();
        self.check_and_update(path_str);

        // TODO refresh as a background thread after x interval
        let cache = self.cache.lock().unwrap();
        let (kind, perms, size, seconds) = match cache.get(path_str) {
            Some(meta) => {
                let kind = if meta.directory {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };

                let seconds = match &meta.file_meta {
                    Some(f) => f.modified_since,
                    None => 0,
                };

                (kind, meta.perms, meta.size, seconds)
            }
            _ => {
                // println!("Not found {}\n{:?}", path_str, cache);
                return Err(libc::ENOSYS);
            }
        };

        let time = |secs: u32| SystemTime::UNIX_EPOCH + Duration::new(secs as u64, 0);

        let attr = FileAttr {
            size,
            blocks: 4096 as u64,
            atime: time(seconds),
            mtime: time(seconds),
            ctime: time(seconds),
            crtime: SystemTime::UNIX_EPOCH,
            kind,
            perm: perms,
            nlink: 1,
            uid: 1,
            gid: 1,
            rdev: 3,
            flags: 0,
        };

        Ok((TTL, attr))
    }

    fn chmod(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: Option<u64>,
        _mode: u32,
    ) -> ResultEmpty {
        println!("chmod");
        Err(libc::ENOSYS)
    }

    fn chown(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: Option<u64>,
        _uid: Option<u32>,
        _gid: Option<u32>,
    ) -> ResultEmpty {
        println!("chown");
        Err(libc::ENOSYS)
    }

    fn truncate(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: Option<u64>,
        _size: u64,
    ) -> ResultEmpty {
        println!("truncate");
        Err(libc::ENOSYS)
    }

    fn utimens(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: Option<u64>,
        _atime: Option<std::time::SystemTime>,
        _mtime: Option<std::time::SystemTime>,
    ) -> ResultEmpty {
        println!("utimens");
        Err(libc::ENOSYS)
    }

    fn utimens_macos(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
    ) -> ResultEmpty {
        println!("utimens");
        Err(libc::ENOSYS)
    }

    fn readlink(&self, _req: RequestInfo, _path: &std::path::Path) -> ResultData {
        println!("readlink");
        Err(libc::ENOSYS)
    }

    fn mknod(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _mode: u32,
        _rdev: u32,
    ) -> ResultEntry {
        println!("mknod");
        Err(libc::ENOSYS)
    }

    fn mkdir(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _mode: u32,
    ) -> ResultEntry {
        println!("mkdir");
        Err(libc::ENOSYS)
    }

    fn unlink(&self, _req: RequestInfo, _parent: &std::path::Path, _name: &OsStr) -> ResultEmpty {
        println!("unlink");
        Err(libc::ENOSYS)
    }

    fn rmdir(&self, _req: RequestInfo, _parent: &std::path::Path, _name: &OsStr) -> ResultEmpty {
        println!("rmdir");
        Err(libc::ENOSYS)
    }

    fn symlink(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _target: &std::path::Path,
    ) -> ResultEntry {
        println!("symlink");
        Err(libc::ENOSYS)
    }

    fn rename(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _newparent: &std::path::Path,
        _newname: &OsStr,
    ) -> ResultEmpty {
        println!("rename");
        Err(libc::ENOSYS)
    }

    fn link(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _newparent: &std::path::Path,
        _newname: &OsStr,
    ) -> ResultEntry {
        println!("link");
        Err(libc::ENOSYS)
    }

    fn open(&self, _req: RequestInfo, _path: &std::path::Path, _flags: u32) -> ResultOpen {
        println!("open");

        /* reading a file requires
        open
        read
        flush
        release
        */

        // TODO read the file and poke it into a open file cache

        // Err(libc::ENOSYS)
        Ok((1, 1))
    }

    fn read(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _offset: u64,
        _size: u32,
        callback: impl FnOnce(ResultSlice<'_>) -> CallbackResult,
    ) -> CallbackResult {
        println!("read");
        callback(Err(libc::ENOSYS))
    }

    fn write(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _offset: u64,
        _data: Vec<u8>,
        _flags: u32,
    ) -> ResultWrite {
        println!("write");
        Err(libc::ENOSYS)
    }

    fn flush(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _lock_owner: u64,
    ) -> ResultEmpty {
        println!("flush");
        Err(libc::ENOSYS)
    }

    fn release(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> ResultEmpty {
        println!("release");
        Err(libc::ENOSYS)
    }

    fn fsync(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _datasync: bool,
    ) -> ResultEmpty {
        println!("fsync");
        Err(libc::ENOSYS)
    }

    fn opendir(&self, _req: RequestInfo, path: &std::path::Path, _flags: u32) -> ResultOpen {
        println!("opendir {:?}", path);

        let path_str = path.to_str().unwrap();
        self.check_and_update(path_str);

        let cache = self.cache.lock().unwrap();

        if cache.contains_key(path_str) {
            // return okay so cd doesn't fail
            Ok((1, 1))
        } else {
            Err(libc::ENOSYS)
        }
    }

    fn readdir(&self, _req: RequestInfo, path: &std::path::Path, _fh: u64) -> ResultReaddir {
        // println!("readdir: {:?} {:?} {:?}", _req, path, _fh);
        let entries = self.get_entries(path);
        Ok(entries)
    }

    fn releasedir(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _flags: u32,
    ) -> ResultEmpty {
        println!("releasedir");
        Err(libc::ENOSYS)
    }

    fn fsyncdir(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _fh: u64,
        _datasync: bool,
    ) -> ResultEmpty {
        println!("fsyncdir");
        Err(libc::ENOSYS)
    }

    fn statfs(&self, _req: RequestInfo, _path: &std::path::Path) -> ResultStatfs {
        // println!("statfs");
        // Err(libc::ENOSYS)

        Ok(Statfs {
            blocks: 0 as u64,
            bfree: 0 as u64,
            bavail: 0 as u64,
            files: 0 as u64,
            ffree: 0 as u64,
            bsize: 0 as u32,
            namelen: 0 as u32,
            frsize: 0 as u32,
        })
    }

    fn setxattr(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _name: &OsStr,
        _value: &[u8],
        _flags: u32,
        _position: u32,
    ) -> ResultEmpty {
        println!("setxattr");
        Err(libc::ENOSYS)
    }

    fn getxattr(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _name: &OsStr,
        _size: u32,
    ) -> ResultXattr {
        println!("getxattr");

        Err(libc::ENOSYS)
    }

    fn listxattr(&self, _req: RequestInfo, _path: &std::path::Path, _size: u32) -> ResultXattr {
        println!("listxattr");
        Err(libc::ENOSYS)
    }

    fn removexattr(
        &self,
        _req: RequestInfo,
        _path: &std::path::Path,
        _name: &OsStr,
    ) -> ResultEmpty {
        println!("removexattr");
        Err(libc::ENOSYS)
    }

    fn access(&self, _req: RequestInfo, _path: &std::path::Path, _mask: u32) -> ResultEmpty {
        println!("access");
        Err(libc::ENOSYS)
    }

    fn create(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> ResultCreate {
        println!("create");
        Err(libc::ENOSYS)
    }

    fn setvolname(&self, _req: RequestInfo, _name: &OsStr) -> ResultEmpty {
        println!("setvolname");
        Err(libc::ENOSYS)
    }

    fn getxtimes(&self, _req: RequestInfo, _path: &std::path::Path) -> ResultXTimes {
        println!("getxtimes");
        Err(libc::ENOSYS)
    }
}
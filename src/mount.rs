use crate::cmd::CmdRunner;
use crate::ls::FileMeta;
use fuse_mt::*;
use libc;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::str;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::SystemTime;
use std::{collections::HashMap, path::PathBuf};
use std::{ffi::OsStr, time::Instant};
use std::{
    fs,
    sync::atomic::{AtomicU32, Ordering},
};

const TTL: Duration = Duration::from_secs(60);

/// helper to mount a path
pub fn mount(runner: impl CmdRunner + 'static) {
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

impl Default for CachedMeta {
    fn default() -> Self {
        Self {
            file_meta: Default::default(),
            directory: Default::default(),
            perms: 0o7777,
            size: Default::default(),
            children: Default::default(),
            updated: Default::default(),
            last_updated: Instant::now(),
        }
    }
}

struct CachedFile {
    contents: Vec<u8>,
    last_updated: Instant,
}

/// this is a file system back by a cache built on the fly from a remote
/// listing. the list command is currently on done on the parent and hence
/// would not have complete data. Ideally this could be merged from stat
/// information
struct SshFuseFs<T> {
    runner: T,
    /// filesystem metadata cache
    cache: Arc<Mutex<HashMap<String, CachedMeta>>>,
    /// file cache
    file_cache: Arc<Mutex<HashMap<String, CachedFile>>>,

    counter: AtomicU32,
}

impl<T: CmdRunner + Sync + Send> SshFuseFs<T> {
    fn new(runner: T) -> Self {
        // let trace_bar = get_progress_bar(&views);

        SshFuseFs {
            runner,
            cache: Default::default(),
            file_cache: Default::default(),

            // trace_bar,
            counter: Default::default(),
        }
    }

    fn get_key(key: &str) -> &str {
        // keys are stored without trailing slashes
        let key = if key == "/" { "" } else { key };

        assert!(!key.ends_with("/"));

        key
    }

    /// based on a key path, check the cache,
    /// otherwise fetch a file/directory metadata
    /// used by getattr and opendir
    fn get_or_update_metadata(&self, path_str: &str) {
        let mut buf = PathBuf::from(path_str);
        buf.pop();
        let parent_path = buf.to_str().unwrap();

        let in_cache = {
            let cache = self.cache.lock().unwrap();

            if cache.contains_key(Self::get_key(path_str)) {
                true
            } else {
                // if parent's listing is updated, use the cache!
                match cache.get(Self::get_key(parent_path)) {
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
        self.runner.fetch_path(path)
    }

    /// this uses 2 path parameters
    /// the path without slash is used to populate the cache's keys
    /// path with the forward slash is to force `ls` to list the directory
    /// content and not just the path
    fn update_dir_cache(&self, path: &str, no_trailing_key: &str) {
        // for root "/", the key is ""
        let no_trailing_key = if path == "/" { "" } else { no_trailing_key };

        let meta = match self.fetch_path(path) {
            Some(meta) => meta,
            _ => return,
        };

        let mut cache = self.cache.lock();
        let cache = cache.as_mut().unwrap();

        // populate cache
        let children = meta.iter().map(|m| m.name.to_string()).collect::<Vec<_>>();

        let parent = cache.entry(no_trailing_key.into()).or_default();

        parent.updated = true;
        parent.children = Some(children);
        parent.directory = true;

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

    /// attempts to get directory listing from cache, other make a fetch
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

            let cached = cache.get(&no_trailing_key);
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
        let cached = cache.get(&no_trailing_key).unwrap();

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

    /// use this for tracking or logging syscalls
    fn track(&self, syscall: &str, path: &Path) {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        if count % 10 == 0 {
            // self.trace_bar
            println!("{}", format!("syscall {}: {} {:?}", count, syscall, path));
        }
    }
}

impl<T: CmdRunner> FilesystemMT for SshFuseFs<T> {
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        self.track("init", &Path::new(""));
        Ok(())
    }

    fn destroy(&self, _req: RequestInfo) {
        self.track("destroy", &Path::new(""));
        // Nothing.
    }

    fn getattr(&self, _req: RequestInfo, path: &std::path::Path, _fh: Option<u64>) -> ResultEntry {
        self.track("getattr", path);

        let path_str = path.to_str().unwrap();
        self.get_or_update_metadata(path_str);

        // TODO refresh as a background thread after x interval
        let cache = self.cache.lock().unwrap();
        let (kind, perms, size, seconds) = match cache.get(Self::get_key(path_str)) {
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
            rdev: 0,
            flags: 0,
        };

        Ok((TTL, attr))
    }

    fn chmod(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        _mode: u32,
    ) -> ResultEmpty {
        self.track("chmod", path);
        Err(libc::ENOSYS)
    }

    fn chown(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        _uid: Option<u32>,
        _gid: Option<u32>,
    ) -> ResultEmpty {
        self.track("chown", path);
        Err(libc::ENOSYS)
    }

    fn truncate(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        _size: u64,
    ) -> ResultEmpty {
        self.track("truncate", path);
        Err(libc::ENOSYS)
    }

    fn utimens(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        _atime: Option<std::time::SystemTime>,
        _mtime: Option<std::time::SystemTime>,
    ) -> ResultEmpty {
        self.track("utimens", path);
        Err(libc::ENOSYS)
    }

    fn utimens_macos(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
    ) -> ResultEmpty {
        self.track("utimens", path);
        Err(libc::ENOSYS)
    }

    fn readlink(&self, _req: RequestInfo, path: &std::path::Path) -> ResultData {
        self.track("readlink", path);
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
        self.track("mknod", _parent);
        Err(libc::ENOSYS)
    }

    fn mkdir(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _mode: u32,
    ) -> ResultEntry {
        self.track("mkdir", _parent);
        Err(libc::ENOSYS)
    }

    fn unlink(&self, _req: RequestInfo, _parent: &std::path::Path, _name: &OsStr) -> ResultEmpty {
        self.track("unlink", _parent);
        Err(libc::ENOSYS)
    }

    fn rmdir(&self, _req: RequestInfo, _parent: &std::path::Path, _name: &OsStr) -> ResultEmpty {
        self.track("rmdir", _parent);
        Err(libc::ENOSYS)
    }

    fn symlink(
        &self,
        _req: RequestInfo,
        _parent: &std::path::Path,
        _name: &OsStr,
        _target: &std::path::Path,
    ) -> ResultEntry {
        self.track("symlink", _parent);
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
        self.track("rename", _parent);
        Err(libc::ENOSYS)
    }

    fn link(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _newparent: &std::path::Path,
        _newname: &OsStr,
    ) -> ResultEntry {
        self.track("link", path);
        Err(libc::ENOSYS)
    }

    fn open(&self, _req: RequestInfo, path: &std::path::Path, _flags: u32) -> ResultOpen {
        self.track("open", path);
        let path = path.to_str().unwrap();

        let mut cache = self.file_cache.lock().unwrap();
        if cache.contains_key(path) {
            return Ok((1, 1));
        }
        let output = self.runner.fetch_file(path);

        if output.stderr.len() > 0 {
            return Err(libc::ENOSYS);
        }

        let file = CachedFile {
            contents: output.stdout,
            last_updated: Instant::now(),
        };

        cache.insert(path.into(), file);

        /* reading a file requires
        open
        read
        flush
        release
        */
        Ok((1, 1))
    }

    fn read(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        offset: u64,
        size: u32,
        callback: impl FnOnce(ResultSlice<'_>) -> CallbackResult,
    ) -> CallbackResult {
        self.track("read", path);
        let path = path.to_str().unwrap();
        // println!("read {} offset {} size {}", path, offset, size);

        let file_cache = self.file_cache.lock().unwrap();
        let file = match file_cache.get(path) {
            Some(file) => file,
            _ => {
                return callback(Err(libc::ENOENT)); // EACCES
            }
        };

        let contents = &file.contents;
        let slice =
            &contents[offset as usize..(offset as usize + size as usize).min(contents.len())];

        callback(Ok(slice))
    }

    fn write(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _offset: u64,
        _data: Vec<u8>,
        _flags: u32,
    ) -> ResultWrite {
        self.track("write", path);
        Err(libc::ENOSYS)
    }

    fn flush(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _lock_owner: u64,
    ) -> ResultEmpty {
        self.track("flush", path);
        Err(libc::ENOSYS)
    }

    fn release(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> ResultEmpty {
        self.track("release", path);
        Err(libc::ENOSYS)
    }

    fn fsync(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _datasync: bool,
    ) -> ResultEmpty {
        self.track("fsync", path);
        Err(libc::ENOSYS)
    }

    fn opendir(&self, _req: RequestInfo, path: &std::path::Path, _flags: u32) -> ResultOpen {
        self.track("opendir", path);

        let path_str = path.to_str().unwrap();
        self.get_or_update_metadata(path_str);

        let cache = self.cache.lock().unwrap();

        if cache.contains_key(Self::get_key(path_str)) {
            // return okay so cd doesn't fail
            Ok((1, 1))
        } else {
            // not a file or directory!
            Err(libc::ENOENT)
        }
    }

    // we optimistically think the directory should be preload in cache!
    fn readdir(&self, _req: RequestInfo, path: &std::path::Path, _fh: u64) -> ResultReaddir {
        self.track("readdir", path);
        let entries = self.get_entries(path);
        Ok(entries)
    }

    fn releasedir(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _flags: u32,
    ) -> ResultEmpty {
        self.track("releasedir", path);
        Err(libc::ENOSYS)
    }

    fn fsyncdir(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _datasync: bool,
    ) -> ResultEmpty {
        self.track("fsyncdir", path);
        Err(libc::ENOSYS)
    }

    fn statfs(&self, _req: RequestInfo, path: &std::path::Path) -> ResultStatfs {
        self.track("fsyncdir", path);

        // we need to return something!

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
        path: &std::path::Path,
        _name: &OsStr,
        _value: &[u8],
        _flags: u32,
        _position: u32,
    ) -> ResultEmpty {
        self.track("setxattr", path);
        Err(libc::ENOSYS)
    }

    fn getxattr(
        &self,
        _req: RequestInfo,
        path: &std::path::Path,
        _name: &OsStr,
        _size: u32,
    ) -> ResultXattr {
        self.track("getxattr", path);

        Err(libc::ENOSYS)
    }

    fn listxattr(&self, _req: RequestInfo, path: &std::path::Path, _size: u32) -> ResultXattr {
        self.track("listxattr", path);
        Err(libc::ENOSYS)
    }

    fn removexattr(&self, _req: RequestInfo, path: &std::path::Path, _name: &OsStr) -> ResultEmpty {
        self.track("removexattr", path);
        Err(libc::ENOSYS)
    }

    fn access(&self, _req: RequestInfo, path: &std::path::Path, _mask: u32) -> ResultEmpty {
        self.track("access", path);
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
        self.track("create", _parent);
        Err(libc::ENOSYS)
    }

    fn setvolname(&self, _req: RequestInfo, _name: &OsStr) -> ResultEmpty {
        self.track("setvolname", &Path::new(""));
        Err(libc::ENOSYS)
    }

    fn getxtimes(&self, _req: RequestInfo, path: &std::path::Path) -> ResultXTimes {
        self.track("getxtimes", path);
        Err(libc::ENOSYS)
    }
}

#[test]
fn test_runner() {
    use crate::ls::parse_long_list;
    use std::process::Output;

    struct TestRunner {
        count: AtomicU32,
    }

    impl CmdRunner for TestRunner {
        fn fetch_path(&self, path: &str) -> Option<Vec<FileMeta>> {
            println!("fetch_path {}", path);
            self.count.fetch_add(1, Ordering::Relaxed);
            match path {
                "/" => {
                    let ls = r"total 128
                    drwxr-xr-x   2 root root  4096 Mar  3 23:27 bin
                    drwxr-xr-x   3 root root  4096 Jun 25 06:00 boot
                    drwxr-xr-x  14 root root  3160 Dec 17  2020 dev
                    drwxr-xr-x 105 root root  4096 Jun 25 21:26 etc";
                    Some(parse_long_list(ls))
                }
                "/boot/" => {
                    let ls = r"total 128M
                    -rw------- 1 root root 3.7M Jul  4  2019 System.map-4.15.0-1044-aws
                    -rw------- 1 root root 3.7M Nov  7  2019 System.map-4.15.0-1054-aws
                    -rw------- 1 root root 4.3M May 14 16:08 System.map-5.4.0-1049-aws";
                    Some(parse_long_list(ls))
                }
                _ => None,
            }
        }

        fn fetch_file(&self, path: &str) -> Output {
            todo!();
        }
    }

    let runner = TestRunner {
        count: Default::default(),
    };
    let filesystem = SshFuseFs::new(runner);

    assert_eq!(filesystem.cache.lock().unwrap().contains_key(""), false);
    assert_eq!(filesystem.runner.count.load(Ordering::Relaxed), 0);

    filesystem.get_or_update_metadata("/");
    assert_eq!(filesystem.cache.lock().unwrap().contains_key(""), true);
    assert_eq!(filesystem.runner.count.load(Ordering::Relaxed), 1);

    // make sure that it's reading from cache
    filesystem.get_or_update_metadata("/");
    assert_eq!(filesystem.runner.count.load(Ordering::Relaxed), 1);
    // println!("cache: {:#?}", filesystem.cache);

    // still reading from cache but only attrs are needed, could spin
    // things up in the background
    filesystem.get_or_update_metadata("/boot");
    assert_eq!(filesystem.runner.count.load(Ordering::Relaxed), 1);

    assert_eq!(filesystem.get_dir_list_from_cache("/").len(), 4);

    assert_eq!(filesystem.get_dir_list_from_cache("/boot").len(), 3);
    assert_eq!(filesystem.runner.count.load(Ordering::Relaxed), 2);
}

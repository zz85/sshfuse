use std::str;

use chrono::{Datelike, NaiveDate, Utc};

#[derive(Debug, Clone, Default)]
pub struct FileMeta {
    pub directory: bool,
    pub permissions: String,
    pub perms: u16,
    pub links: u16,
    pub owner_name: String,
    pub owner_group: String,
    pub file_size: usize,
    pub month: String,
    pub date: String,
    pub time_year: String,
    pub name: String,
    pub modified_since: u32,
}

pub fn parse_long_list(ls: &str) -> Vec<FileMeta> {
    let lines = ls.split('\n');

    let dir = lines
        .into_iter()
        .filter_map(parse_long_list_line)
        .collect::<Vec<_>>();

    dir
}

fn parse_long_list_line(line: &str) -> Option<FileMeta> {
    let mut split = line.trim().split_whitespace();

    let permissions = split.next()?.to_string();
    let links: u16 = split.next()?.parse().ok()?;
    let owner_name = split.next()?.to_string();
    let owner_group = split.next()?.to_string();
    let file_size: usize = split.next()?.parse().unwrap_or(0);
    let month = split.next()?.to_string();
    let date = split.next()?.to_string();
    let time_year = split.next()?.to_string(); // eg. 15:01 / 2018
    let rest = split.collect::<Vec<_>>().join(" ");

    let mut chars = permissions.chars();
    let first_char = chars.next();
    let is_link = first_char == Some('l');

    let directory = match first_char {
        Some('d') => true,
        Some('l') => true, // we fake symbolic links as directories for now to avoid an additional lookup
        _ => false,
    };

    let perms = 0u16
        + (permissions_octet(&mut chars) << 6)
        + (permissions_octet(&mut chars) << 3)
        + permissions_octet(&mut chars);

    let perms = if !is_link { perms } else { 0o7777 };

    let name = if is_link {
        rest.split(" -> ").next().unwrap().to_string()
    } else {
        rest.to_string()
    };

    let modified_since = parse_time(&month, &date, &time_year)?;

    Some(FileMeta {
        directory,
        permissions,
        perms,
        links,
        owner_name,
        owner_group,
        file_size,
        month,
        date,
        time_year,
        name,
        modified_since,
    })
}

fn permissions_octet(chars: &mut str::Chars) -> u16 {
    let mut v = 0;
    match chars.next() {
        Some('r') => {
            v += 4;
        }
        _ => {}
    }
    match chars.next() {
        Some('w') => {
            v += 2;
        }
        _ => {}
    }
    match chars.next() {
        Some('x') => {
            v += 1;
        }
        _ => {}
    }
    v
}

#[test]
fn test_ubuntu() {
    let sample = r"total 128
    drwxr-xr-x   2 root root  4096 Mar  3 23:27 bin
    drwxr-xr-x   3 root root  4096 Jun 25 06:00 boot
    drwxr-xr-x  14 root root  3160 Dec 17  2020 dev
    drwxr-xr-x 105 root root  4096 Jun 25 21:26 etc
    drwxr-xr-x   3 root root  4096 Jul 31  2019 home
    lrwxrwxrwx   1 root root    30 Jun 24 06:39 initrd.img -> boot/initrd.img-5.4.0-1051-aws
    lrwxrwxrwx   1 root root    30 Jun 24 06:39 initrd.img.old -> boot/initrd.img-5.4.0-1049-aws
    drwxr-xr-x  21 root root  4096 Jan  6 11:28 lib
    drwxr-xr-x   2 root root  4096 Jul  7  2020 lib32
    drwxr-xr-x   2 root root  4096 Jul  7  2020 lib64
    drwx------   2 root root 16384 Jul 22  2019 lost+found
    drwxr-xr-x   2 root root  4096 Jul 22  2019 media
    drwxr-xr-x   2 root root  4096 Jul 22  2019 mnt
    drwxr-xr-x   2 root root  4096 Jul 22  2019 opt
    dr-xr-xr-x 532 root root     0 Nov 21  2019 proc
    drwx------   4 root root  4096 Jun 12 21:13 root
    drwxr-xr-x  30 root root  1120 Jun 27 15:19 run
    drwxr-xr-x   2 root root 12288 May 29 06:21 sbin
    drwxr-xr-x   8 root root  4096 Nov  6  2020 snap
    drwxr-xr-x   2 root root  4096 Jul 22  2019 srv
    dr-xr-xr-x  13 root root     0 Jun 26 21:55 sys
    drwxrwxrwt 149 root root 36864 Jun 27 14:31 tmp
    drwxr-xr-x  11 root root  4096 Mar 15  2020 usr
    drwxr-xr-x  13 root root  4096 Jul 22  2019 var
    lrwxrwxrwx   1 root root    27 Jun 24 06:39 vmlinuz -> boot/vmlinuz-5.4.0-1051-aws
    lrwxrwxrwx   1 root root    27 Jun 24 06:39 vmlinuz.old -> boot/vmlinuz-5.4.0-1049-aws
    ";

    let dir = parse_long_list(sample);

    assert_eq!(dir.len(), 26);

    // this not technically right because of the symlinks
    assert_eq!(
        dir.iter().filter(|m| m.directory).collect::<Vec<_>>().len(),
        26
    );
}

#[test]
fn test_parse_err() {
    let sample = r"ls: cannot access '/fdasfksahfjkdsa': No such file or directory";

    let dir = parse_long_list(sample);
    assert_eq!(dir.len(), 0);
}

#[test]
fn test_mac() {
    let sample = r"total 48
    -rw-r--r--  1 zz85  staff   6.7K 26 Jun 19:08 Cargo.lock
    -rw-r--r--  1 zz85  staff   345B 26 Jun 19:08 Cargo.toml
    -rw-r--r--  1 zz85  staff   1.0K 26 Jun 13:41 LICENSE
    -rw-r--r--  1 zz85  staff   611B 27 Jun 00:34 README.md
    drwxr-xr-x  5 zz85  staff   160B 26 Jun 16:59 src
    drwxr-xr-x@ 5 zz85  staff   160B 26 Jun 13:42 target
    -rwxr-xr-x  1 zz85  staff   128B 26 Jun 15:52 test.sh
    ";

    let dir = parse_long_list(sample);

    assert_eq!(dir.len(), 7);
    assert_eq!(
        dir.iter().filter(|m| m.directory).collect::<Vec<_>>().len(),
        2
    );
}

fn parse_month(month: &str) -> Option<u32> {
    let v = match month {
        "Jan" => 0,
        "Feb" => 1,
        "Mar" => 2,
        "Apr" => 3,
        "May" => 4,
        "Jun" => 5,
        "Jul" => 6,
        "Aug" => 7,
        "Sep" => 8,
        "Oct" => 9,
        "Nov" => 10,
        "Dec" => 11,
        _ => return None,
    };
    Some(v + 1)
}

fn parse_time(month: &str, day: &str, time_year: &str) -> Option<u32> {
    let has_time = time_year.contains(':');

    let now = Utc::now();

    let ts = if has_time {
        let day = day.parse().ok()?;
        let month = parse_month(month)?;
        let mut time = time_year.split(':').filter_map(|t| t.parse().ok());
        NaiveDate::from_ymd(now.year(), month, day).and_hms(time.next()?, time.next()?, 0)
    } else {
        let day = day.parse().ok()?;
        let month = parse_month(month)?;
        let year = time_year.parse().ok()?;

        NaiveDate::from_ymd(year, month, day).and_hms(0, 0, 0)
    };

    Some(ts.timestamp() as u32)
}
#[test]
fn test_time_parsing() {
    assert!(parse_time("Jun", "26", "19:08").is_some());
    assert!(parse_time("Jul", "31", "2019").is_some());
}

#[test]
fn test_stat() {
    // TODO parse stat output

    let _stat = r"stat /bin
File: /bin
Size: 4096            Blocks: 8          IO Block: 4096   directory
Device: 10303h/66307d   Inode: 12          Links: 2
Access: (0755/drwxr-xr-x)  Uid: (    0/    root)   Gid: (    0/    root)
Access: 2021-06-26 08:05:02.904641271 +0000
Modify: 2021-03-03 23:27:41.025619169 +0000
Change: 2021-03-03 23:27:41.025619169 +0000
Birth: -";

    let _stat_ln = r"File: /vmlinuz -> boot/vmlinuz-5.4.0-1051-aws
Size: 27              Blocks: 0          IO Block: 4096   symbolic link
Device: 10303h/66307d   Inode: 59264       Links: 1
Access: (0777/lrwxrwxrwx)  Uid: (    0/    root)   Gid: (    0/    root)
Access: 2021-06-26 21:55:07.638695727 +0000
Modify: 2021-06-24 06:39:16.664708918 +0000
Change: 2021-06-24 06:39:16.664708918 +0000
Birth: -";

    let _stat_linked = r"stat /boot/initrd.img-5.4.0-1051-aws
File: /boot/initrd.img-5.4.0-1051-aws
Size: 21246462        Blocks: 41504      IO Block: 4096   regular file
Device: 10303h/66307d   Inode: 146138      Links: 1
Access: (0644/-rw-r--r--)  Uid: (    0/    root)   Gid: (    0/    root)
Access: 2021-06-24 06:39:24.292540575 +0000
Modify: 2021-06-24 06:39:24.260541281 +0000
Change: 2021-06-24 06:39:24.288540664 +0000
Birth: -";
}

#[test]
fn test_perms() {
    let sample = r"total 1
    drwxr-xr-x   2 root root  4096 Mar  3 23:27 bin
    ";

    let dir = parse_long_list(sample);
    assert_eq!(dir.len(), 1);

    let file = &dir[0];

    assert_eq!("drwxr-xr-x", file.permissions);

    // format is tttt|ugs|rwxrwxrwx
    // where tttt = 1000 regular file,  0100 dir, device, fifo...
    // ugs = special file usage
    let bytes: u16 = 0b000_111_101_101;
    let oct: u16 = 0o0755;

    assert_eq!(bytes, oct);
    assert_eq!(bytes, file.perms);
}

use std::str::from_utf8;
use std::result::Result;
use nom::IResult;

#[derive(Debug,PartialEq,Eq)]
pub struct PosixHeader<'a> {
    pub name:     & 'a str,
    pub mode:     & 'a str,
    pub uid:      u64,
    pub gid:      u64,
    pub size:     u64,
    pub mtime:    u64,
    pub chksum:   & 'a str,
    pub typeflag: TypeFlag,
    pub linkname: & 'a str,
    pub ustar:    Option<UStarHeader<'a>>
}

#[derive(Debug,PartialEq,Eq)]
pub struct UStarHeader<'a> {
    pub magic:    & 'a str,
    pub version:  & 'a str,
    pub uname:    & 'a str,
    pub gname:    & 'a str,
    pub devmajor: u64,
    pub devminor: u64,
    pub prefix:   & 'a str,
}

#[derive(Debug,PartialEq,Eq)]
pub struct TarEntry<'a> {
    pub header:   PosixHeader<'a>,
    pub contents: & 'a str
}

/* TODO: support vendor specific + sparse */
#[derive(Debug,PartialEq,Eq)]
pub enum TypeFlag {
    NormalFile,
    HardLink,
    SymbolicLink,
    CharacterSpecial,
    BlockSpecial,
    Directory,
    FIFO,
    ContiguousFile,
    GlobalExtendedHeaderWithMetadata,
    ExtendedHeaderWithMetadataForNext,
    VendorSpecific,
    Invalid
}

pub fn octal_to_u64(s: &str) -> Result<u64, &'static str> {
    let mut u = 0;

    for c in s.chars() {
        if c < '0' || c > '7' {
            return Err("invalid octal string received");
        }
        u *= 8;
        u += (c as u64) - ('0' as u64);
    }

    Ok(u)
}

fn char_to_type_flag(c: char) -> TypeFlag {
    match c {
        '0' | '\0' => TypeFlag::NormalFile,
        '1' => TypeFlag::HardLink,
        '2' => TypeFlag::SymbolicLink,
        '3' => TypeFlag::CharacterSpecial,
        '4' => TypeFlag::BlockSpecial,
        '5' => TypeFlag::Directory,
        '6' => TypeFlag::FIFO,
        '7' => TypeFlag::ContiguousFile,
        'g' => TypeFlag::GlobalExtendedHeaderWithMetadata,
        'x' => TypeFlag::ExtendedHeaderWithMetadataForNext,
        'A' ... 'Z' => TypeFlag::VendorSpecific,
        _ => TypeFlag::Invalid
    }
}

macro_rules! take_str_eat_garbage (
 ( $i:expr, $size:expr ) => ( chain!( $i, s: map_res!(take_until!("\0"), from_utf8) ~ take!($size - s.len()), ||{ s } ));
);

fn parse_ustar00(i: &[u8]) -> IResult<&[u8], Option<UStarHeader>> {
    chain!(i,
        tag!("00")                                                 ~
        uname:    take_str_eat_garbage!(32)                        ~
        gname:    take_str_eat_garbage!(32)                        ~
        devmajor: map_res!(take_str_eat_garbage!(8), octal_to_u64) ~
        devminor: map_res!(take_str_eat_garbage!(8), octal_to_u64) ~
        prefix:   take_str_eat_garbage!(155)                       ~
        take!(12), /* padding to 512 */
        ||{
            Some(UStarHeader {
                magic:    "ustar\0",
                version:  "00",
                uname:    uname,
                gname:    gname,
                devmajor: devmajor,
                devminor: devminor,
                prefix:   prefix
            })
        }
    )
}

fn parse_ustar(i: &[u8]) -> IResult<&[u8], Option<UStarHeader>> {
    chain!(i,
        tag!("ustar\0") ~
        ustar: parse_ustar00,
        ||{
            ustar
        }
    )
}

fn parse_posix(i: &[u8]) -> IResult<&[u8], Option<UStarHeader>> {
    chain!(i,
        take!(255), /* padding to 512 */
        ||{
            None
        }
    )
}

fn parse_header(i: &[u8]) -> IResult<&[u8], PosixHeader> {
    chain!(i,
        name:     take_str_eat_garbage!(100)                        ~
        mode:     take_str_eat_garbage!(8)                          ~
        uid:      map_res!(take_str_eat_garbage!(8),  octal_to_u64) ~
        gid:      map_res!(take_str_eat_garbage!(8),  octal_to_u64) ~
        size:     map_res!(take_str_eat_garbage!(12), octal_to_u64) ~
        mtime:    map_res!(take_str_eat_garbage!(12), octal_to_u64) ~
        chksum:   take_str_eat_garbage!(8)                          ~
        typeflag: take!(1)                                          ~
        linkname: take_str_eat_garbage!(100)                        ~
        ustar:    alt!(parse_ustar | parse_posix),
        ||{
            PosixHeader {
                name:     name,
                mode:     mode,
                uid:      uid,
                gid:      gid,
                size:     size,
                mtime:    mtime,
                chksum:   chksum,
                typeflag: char_to_type_flag(typeflag[0] as char),
                linkname: linkname,
                ustar:    ustar
            }
        }
    )
}

fn parse_contents(i: &[u8], size: u64) -> IResult<&[u8], &str> {
    let trailing = size % 512;
    let padding = match trailing {
        0 => 0,
        t => 512 - t
    };
    chain!(i,
        contents: take_str!(size as usize) ~
        take!(padding as usize),
        ||{
            contents
        }
    )
}

fn parse_entry(i: &[u8]) -> IResult<&[u8], TarEntry> {
    chain!(i,
        header:   parse_header ~
        contents: apply!(parse_contents, header.size),
        ||{
            TarEntry {
                header: header,
                contents: contents
            }
        }
    )
}

fn filter_entries(entries: Vec<TarEntry>) -> Result<Vec<TarEntry>, &'static str> {
    Ok(entries.into_iter().filter(|e| e.header.name != "").collect::<Vec<TarEntry>>())
}

pub fn parse_tar(i: &[u8]) -> IResult<&[u8], Vec<TarEntry>> {
    map_res!(i, many0!(parse_entry), filter_entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::from_utf8;
    use nom::IResult;

    #[test]
    fn octal_to_u64_ok_test() {
        assert_eq!(octal_to_u64("756"), Ok(494));
        assert_eq!(octal_to_u64(""), Ok(0));
    }

    #[test]
    fn octal_to_u64_error_test() {
        assert_eq!(octal_to_u64("1238"), Err("invalid octal string received"));
        assert_eq!(octal_to_u64("a"), Err("invalid octal string received"));
        assert_eq!(octal_to_u64("A"), Err("invalid octal string received"));
    }

    #[test]
    fn take_str_eat_garbage_test() {
        let s = b"foobar\0\0\0\0baz";
        let baz = b"baz";
        assert_eq!(take_str_eat_garbage!(&s[..], 10), IResult::Done(&baz[..], "foobar"));
    }
}

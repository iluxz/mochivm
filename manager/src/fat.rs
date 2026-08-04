//! Minimal FAT32 image builder. Writes a bootable disk containing a single
//! file at EFI/BOOT/BOOTX64.EFI (the mochivm hypervisor app), so OVMF can
//! find and boot it without needing mtools or any external tooling.

use std::io::Write;
use std::path::Path;

const BS: usize = 512;
/// 34MB. Sized so data clusters (>= 65525, the FAT32 minimum that EDK2's
/// FatDxe enforces) fit with 1-sector clusters. Cluster 2 = root dir.
const TOTAL_SEC: u32 = 69632;
const RSVD_SEC: u32 = 32;
const NUM_FATS: u32 = 2;
const FAT_SEC: u32 = 1024;
const DATA_START: u32 = RSVD_SEC + NUM_FATS * FAT_SEC; // 2080
const ROOT_CLUS: u32 = 2;
const EFI_DIR_CLUS: u32 = 3;
const BOOT_DIR_CLUS: u32 = 4;
const FILE_START_CLUS: u32 = 5;

const EOC: u32 = 0x0FFF_FFFF;

fn short_name(s: &str) -> [u8; 11] {
    assert!(s.len() <= 11, "short name too long: {s}");
    let mut n = [b' '; 11];
    n[..s.len()].copy_from_slice(s.as_bytes());
    n
}

/// Build an 8.3 directory-entry name: NAME[8] + EXT[3], no dot.
fn fat_83(name: &[u8; 8], ext: &[u8; 3]) -> [u8; 11] {
    let mut n = [b' '; 11];
    n[..8].copy_from_slice(name);
    n[8..11].copy_from_slice(ext);
    n
}

pub fn write_fat32_disk(efi: &[u8], path: &Path) -> Result<(), String> {
    let mut img = vec![0u8; TOTAL_SEC as usize * BS];

    write_boot_sector(&mut img[..BS]);
    write_fsinfo(&mut img[(1 * BS as u32) as usize..(2 * BS as u32) as usize]);
    write_boot_sector(&mut img[6 * BS..7 * BS]); // backup boot sector
    write_fsinfo(&mut img[7 * BS..8 * BS]); // backup FSInfo
    write_fat(&mut img, efi.len());

    write_dir(
        &mut img,
        ROOT_CLUS,
        &[
            dir_entry(short_name("."), 0x10, ROOT_CLUS, 0),
            dir_entry(short_name(".."), 0x10, ROOT_CLUS, 0),
            dir_entry(short_name("EFI"), 0x10, EFI_DIR_CLUS, 0),
        ],
    );
    write_dir(
        &mut img,
        EFI_DIR_CLUS,
        &[
            dir_entry(short_name("."), 0x10, EFI_DIR_CLUS, 0),
            dir_entry(short_name(".."), 0x10, ROOT_CLUS, 0),
            dir_entry(short_name("BOOT"), 0x10, BOOT_DIR_CLUS, 0),
        ],
    );
    write_dir(
        &mut img,
        BOOT_DIR_CLUS,
        &[
            dir_entry(short_name("."), 0x10, BOOT_DIR_CLUS, 0),
            dir_entry(short_name(".."), 0x10, EFI_DIR_CLUS, 0),
            dir_entry(
                fat_83(b"BOOTX64 ", b"EFI"),
                0x20,
                FILE_START_CLUS,
                efi.len() as u32,
            ),
        ],
    );
    write_file_chain(&mut img, efi);

    std::fs::File::create(path)
        .and_then(|mut f| f.write_all(&img))
        .map_err(|e| format!("failed to write disk image: {e}"))
}

fn write_boot_sector(b: &mut [u8]) {
    b[0] = 0xEB;
    b[1] = 0x3C;
    b[2] = 0x90;
    b[3..11].copy_from_slice(b"MOCHIVM ");
    put16(b, 11, 512); // BytsPerSec
    b[13] = 1; // SecPerClus
    put16(b, 14, RSVD_SEC as u16); // RsvdSecCnt
    b[16] = 2; // NumFATs
    put16(b, 17, 0); // RootEntCnt (0 for FAT32)
    put16(b, 19, 0); // TotSec16
    b[21] = 0xF8; // Media
    put16(b, 22, 0); // FATSz16
    put16(b, 24, 0x20); // SecPerTrk
    put16(b, 26, 0x02); // NumHeads
    put32(b, 28, 0); // HiddSec
    put32(b, 32, TOTAL_SEC); // TotSec32
    put32(b, 36, FAT_SEC); // FATSz32
    put16(b, 40, 0); // ExtFlags
    put16(b, 42, 0); // FSVer
    put32(b, 44, ROOT_CLUS); // RootClus
    put16(b, 48, 1); // FSInfo
    put16(b, 50, 6); // BkBootSec
    b[64] = 0x80; // DrvNum
    b[66] = 0x29; // BootSig
    put32(b, 67, 0x1234_5678); // VolID
    b[71..82].copy_from_slice(b"MOCHIVM    "); // VolLab
    b[82..90].copy_from_slice(b"FAT32   "); // FilSysType
    b[510] = 0x55;
    b[511] = 0xAA;
}

fn write_fsinfo(b: &mut [u8]) {
    put32(b, 0, 0x4161_5252); // "RRaA"
    put32(b, 484, 0x6141_7272); // "rrAa"
    put32(b, 488, TOTAL_SEC - DATA_START); // free cluster count
    put32(b, 492, FILE_START_CLUS); // next free
    put32(b, 508, 0xAA55_0000);
}

fn write_fat(img: &mut [u8], file_len: usize) {
    let nclusters = file_len.div_ceil(BS) as u32;
    for fat_idx in 0..NUM_FATS {
        let fat_start = ((RSVD_SEC + fat_idx * FAT_SEC) * BS as u32) as usize;
        let fat = &mut img[fat_start..fat_start + (FAT_SEC * BS as u32) as usize];
        fat[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
        fat[4..8].copy_from_slice(&EOC.to_le_bytes());
        // root + 2 dirs are single-cluster, chained to EOC
        fat[8..12].copy_from_slice(&EOC.to_le_bytes()); // clus 2
        fat[12..16].copy_from_slice(&EOC.to_le_bytes()); // clus 3
        fat[16..20].copy_from_slice(&EOC.to_le_bytes()); // clus 4
                                                         // file chain
        for i in 0..nclusters {
            let clus = FILE_START_CLUS + i;
            let next = if i + 1 == nclusters { EOC } else { clus + 1 };
            let off = (clus * 4) as usize;
            fat[off..off + 4].copy_from_slice(&next.to_le_bytes());
        }
    }
}

fn dir_entry(name: [u8; 11], attr: u8, first_clus: u32, size: u32) -> [u8; 32] {
    let mut e = [0u8; 32];
    e[0..11].copy_from_slice(&name);
    e[11] = attr;
    put16(&mut e, 20, (first_clus >> 16) as u16); // FstClusHI
    put16(&mut e, 26, first_clus as u16); // FstClusLO
    put32(&mut e, 28, size); // FileSize
    e
}

fn write_dir(img: &mut [u8], clus: u32, entries: &[[u8; 32]]) {
    let mut buf = vec![0u8; BS];
    for (i, e) in entries.iter().enumerate() {
        buf[i * 32..(i + 1) * 32].copy_from_slice(e);
    }
    write_cluster(img, clus, &buf);
}

fn write_file_chain(img: &mut [u8], data: &[u8]) {
    let mut offset = 0usize;
    let mut clus = FILE_START_CLUS;
    loop {
        let sec = sector_of(clus);
        let start = sec as usize * BS;
        let chunk = &data[offset..core::cmp::min(offset + BS, data.len())];
        img[start..start + chunk.len()].copy_from_slice(chunk);
        offset += BS;
        if offset >= data.len() {
            break;
        }
        clus += 1;
    }
}

fn write_cluster(img: &mut [u8], clus: u32, data: &[u8]) {
    let start = sector_of(clus) as usize * BS;
    img[start..start + data.len()].copy_from_slice(data);
}

fn sector_of(clus: u32) -> u32 {
    DATA_START + (clus - ROOT_CLUS) * 1
}

fn put16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootsector_layout() {
        let mut img = vec![0u8; TOTAL_SEC as usize * BS];
        write_boot_sector(&mut img[..BS]);
        assert_eq!(&img[0..3], &[0xEB, 0x3C, 0x90]);
        assert_eq!(u16::from_le_bytes([img[11], img[12]]), 512);
        assert_eq!(u16::from_le_bytes([img[14], img[15]]), RSVD_SEC as u16);
        assert_eq!(
            u32::from_le_bytes(img[32..36].try_into().unwrap()),
            TOTAL_SEC
        );
        assert_eq!(
            u32::from_le_bytes(img[44..48].try_into().unwrap()),
            ROOT_CLUS
        );
        assert_eq!(u16::from_le_bytes([img[48], img[49]]), 1); // FSInfo
        assert_eq!(img[510], 0x55);
        assert_eq!(img[511], 0xAA);
        // EDK2 FatDxe rejects FAT32 volumes with fewer than 65525 clusters
        let max_cluster = TOTAL_SEC - DATA_START;
        assert!(max_cluster >= 65525, "max_cluster={max_cluster} < 65525");
    }

    #[test]
    fn fat83_name_has_no_dot() {
        assert_eq!(fat_83(b"BOOTX64 ", b"EFI"), *b"BOOTX64 EFI");
        assert_eq!(&fat_83(b"BOOTX64 ", b"EFI")[..8], b"BOOTX64 ");
        assert_eq!(&fat_83(b"BOOTX64 ", b"EFI")[8..], b"EFI");
    }

    #[test]
    fn file_lands_at_expected_sector() {
        let efi = vec![0xAAu8; 1024];
        let mut img = vec![0u8; TOTAL_SEC as usize * BS];
        write_fat(&mut img, efi.len());
        write_file_chain(&mut img, &efi);
        let start = sector_of(FILE_START_CLUS) as usize * BS;
        assert_eq!(&img[start..start + 512], &efi[..512]);
        assert_eq!(&img[start + 512..start + 1024], &efi[512..]);

        let fat = &img[RSVD_SEC as usize * BS..];
        assert_eq!(
            u32::from_le_bytes(fat[0..4].try_into().unwrap()),
            0x0FFF_FFF8
        );
        assert_eq!(u32::from_le_bytes(fat[4..8].try_into().unwrap()), EOC);
        assert_eq!(
            u32::from_le_bytes(fat[20..24].try_into().unwrap()),
            FILE_START_CLUS + 1
        );
        assert_eq!(u32::from_le_bytes(fat[24..28].try_into().unwrap()), EOC);
    }
}

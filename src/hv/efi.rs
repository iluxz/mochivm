/// Minimal hand-rolled EFI boot-services bindings. We only need memory
/// allocation, so we poke two structs from the UEFI spec instead of pulling
/// in a full UEFI crate.

#[repr(C)]
struct EfiTableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

#[repr(C)]
struct EfiSystemTable {
    hdr: EfiTableHeader,
    firmware_vendor: u64,
    firmware_revision: u32,
    _pad: u32,
    con_in: u64,
    con_in_ex: u64,
    con_out: u64,
    stderr: u64,
    runtime_services: u64,
    boot_services: u64,
    number_of_table_entries: u64,
    configuration_table: u64,
}

#[repr(C)]
struct EfiBootServices {
    hdr: EfiTableHeader,
    raise_tpl: u64,
    restore_tpl: u64,
    allocate_pages: u64,
    free_pages: u64,
    get_memory_map: u64,
}

type AllocatePagesFn = unsafe extern "win64" fn(u32, u32, u64, *mut u64) -> u64;

/// Allocate `pages` contiguous physical pages (EfiLoaderData, AnyPages).
/// Returns a 4K-aligned pointer into identity-mapped RAM.
pub unsafe fn allocate_pages(pages: u64) -> Result<*mut u8, &'static str> {
    let st = crate::SYSTEM_TABLE as *const EfiSystemTable;
    if st.is_null() {
        return Err("no system table");
    }
    let bs = (*st).boot_services as *const EfiBootServices;
    let allocate: AllocatePagesFn = core::mem::transmute((*bs).allocate_pages);
    let mut addr: u64 = 0;
    // EfiAllocateAnyPages = 0, EfiLoaderData = 7
    let status = allocate(0, 7, pages, &mut addr);
    if status != 0 {
        return Err("allocate_pages failed");
    }
    Ok(addr as *mut u8)
}

pub unsafe fn allocate_page() -> Result<*mut u8, &'static str> {
    allocate_pages(1)
}

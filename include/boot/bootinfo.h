#ifndef OPL_BOOT_BOOTINFO_H
#define OPL_BOOT_BOOTINFO_H

/* Bootloader -> kernel contract. Single source of truth: the Rust mirror in
 * src/kernel/bootinfo.rs must match this layout byte for byte. */

#include <stdint.h>

#define OPL_BOOT_MAGIC   0x00544F4F424C504FULL /* "OPLBOOT\0" little-endian */
#define OPL_BOOT_VERSION 1u

#define OPL_FB_NONE 0u
#define OPL_FB_RGBX 1u /* byte order R,G,B,x */
#define OPL_FB_BGRX 2u /* byte order B,G,R,x */

typedef struct OplBootInfo {
  uint64_t magic;
  uint32_t version;
  uint32_t size; /* sizeof(OplBootInfo) */

  /* Raw UEFI memory map (array of EFI_MEMORY_DESCRIPTOR, stride = desc_size). */
  uint64_t mmap;
  uint64_t mmap_size;
  uint64_t mmap_desc_size;
  uint32_t mmap_desc_version;
  uint32_t _pad0;

  /* Linear framebuffer; fb_format == OPL_FB_NONE if unavailable. */
  uint64_t fb_base;
  uint64_t fb_size;
  uint32_t fb_width;
  uint32_t fb_height;
  uint32_t fb_pitch; /* bytes per scanline */
  uint32_t fb_format;

  uint64_t acpi_rsdp; /* 0 if not found */
  uint64_t smbios;    /* SMBIOS3 (preferred) or SMBIOS2 entry point, 0 if none */
  uint32_t smbios_v3; /* 1 if `smbios` is a 64-bit (_SM3_) entry point */
  uint32_t _pad1;
} OplBootInfo;

#endif

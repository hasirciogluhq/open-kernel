
#include <efi.h>
#include <protocol/efi-sfsp.h>
#include <protocol/efi-gop.h>
#include <boot/bootinfo.h>
#ifndef TRUE
#define TRUE 1
#endif

#ifndef FALSE
#define FALSE 0
#endif

#ifndef NULL
#define NULL ((void *)0)
#endif

#define EFI_ERROR(s) (((INTN)(s)) < 0)
#define EFI_BUFFER_TOO_SMALL ((EFI_STATUS)0x8000000000000005ULL)
#define EFI_INVALID_PARAMETER ((EFI_STATUS)0x8000000000000002ULL)

#define PAGE_SIZE 4096ULL
#define ELF_PT_LOAD 1
#define ELF_PF_X 1
#define ELF_ET_EXEC 2
#define ELF_EM_X86_64 62

#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define MIN(a, b) ((a) < (b) ? (a) : (b))

typedef OplBootInfo BOOT_INFO;

static EFI_GUID GopGuid = EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID;
static EFI_GUID Acpi20Guid = {0x8868E871, 0xE4F1, 0x11D3, {0xBC, 0x22, 0x00, 0x80, 0xC7, 0x3C, 0x88, 0x81}};

static EFI_GUID Smbios3Guid = {0xf2fd1544, 0x9794, 0x4a2c, {0x99, 0x2e, 0xe5, 0xbb, 0xcf, 0x20, 0xe3, 0x94}};
static EFI_GUID SmbiosGuid = {0xeb9d2d31, 0x2d88, 0x11d3, {0x9a, 0x16, 0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d}};

static BOOLEAN guid_eq(const EFI_GUID *x, const EFI_GUID *y) {
  const UINT8 *a = (const UINT8 *)x, *b = (const UINT8 *)y;
  for (UINTN i = 0; i < sizeof(EFI_GUID); i++)
    if (a[i] != b[i]) return FALSE;
  return TRUE;
}

typedef struct {
  UINT8 Ident[16];
  UINT16 Type;
  UINT16 Machine;
  UINT32 Version;
  UINT64 Entry;
  UINT64 PhOff;
  UINT64 ShOff;
  UINT32 Flags;
  UINT16 EhSize;
  UINT16 PhEntSize;
  UINT16 PhNum;
  UINT16 ShEntSize;
  UINT16 ShNum;
  UINT16 ShStrNdx;
} __attribute__((packed)) ELF64_HEADER;

typedef struct {
  UINT32 Type;
  UINT32 Flags;
  UINT64 Offset;
  UINT64 VAddr;
  UINT64 PAddr;
  UINT64 FileSize;
  UINT64 MemSize;
  UINT64 Align;
} __attribute__((packed)) ELF64_PHDR;

typedef struct {
  UINT32 Revision;
  EFI_HANDLE ParentHandle;
  EFI_SYSTEM_TABLE *SystemTable;
  EFI_HANDLE DeviceHandle;
  VOID *FilePath;
  VOID *Reserved;
  UINT32 LoadOptionsSize;
  VOID *LoadOptions;
  VOID *ImageBase;
  UINT64 ImageSize;
  EFI_MEMORY_TYPE ImageCodeType;
  EFI_MEMORY_TYPE ImageDataType;
  VOID *Unload;
} LOADED_IMAGE;

typedef void(__attribute__((sysv_abi)) * KERNEL_ENTRY)(BOOT_INFO *);

static EFI_GUID LoadedImageGuid = {
    0x5B1B31A1,
    0x9562,
    0x11D2,
    {0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B}};

static EFI_GUID FileSystemGuid = EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID;

static EFI_GUID FileInfoGuid = EFI_FILE_INFO_ID;

static EFI_STATUS load_elf(EFI_BOOT_SERVICES *bs, UINT8 *image,
                           UINTN image_size, UINT64 *entry) {
  if (image_size < sizeof(ELF64_HEADER))
    return EFI_INVALID_PARAMETER;

  ELF64_HEADER *eh = (ELF64_HEADER *)image;

  if (eh->Ident[0] != 0x7F || eh->Ident[1] != 'E' || eh->Ident[2] != 'L' ||
      eh->Ident[3] != 'F' || eh->Ident[4] != 2 || eh->Ident[5] != 1 ||
      eh->Type != ELF_ET_EXEC || eh->Machine != ELF_EM_X86_64 ||
      eh->PhEntSize != sizeof(ELF64_PHDR) || eh->PhNum == 0)
    return EFI_INVALID_PARAMETER;

  if (eh->PhOff > image_size ||
      eh->PhNum > (image_size - eh->PhOff) / sizeof(ELF64_PHDR))
    return EFI_INVALID_PARAMETER;

  ELF64_PHDR *phdrs = (ELF64_PHDR *)(image + eh->PhOff);

  UINT64 min_addr = ~(UINT64)0;
  UINT64 max_addr = 0;
  BOOLEAN executable_entry = FALSE;

  for (UINTN i = 0; i < eh->PhNum; i++) {
    ELF64_PHDR *p = &phdrs[i];

    if (p->Type != ELF_PT_LOAD || p->MemSize == 0)
      continue;

    if (p->FileSize > p->MemSize || p->Offset > image_size ||
        p->FileSize > image_size - p->Offset)
      return EFI_INVALID_PARAMETER;

    UINT64 address = p->PAddr ? p->PAddr : p->VAddr;

    // Initial loader uses identity-mapped physical addresses.
    if (address != p->VAddr || address > ~(UINT64)0 - p->MemSize)
      return EFI_INVALID_PARAMETER;

    UINT64 start = address & ~(PAGE_SIZE - 1);
    UINT64 end = address + p->MemSize;

    if (end > ~(UINT64)0 - (PAGE_SIZE - 1))
      return EFI_INVALID_PARAMETER;

    end = (end + PAGE_SIZE - 1) & ~(PAGE_SIZE - 1);

    min_addr = MIN(min_addr, start);
    max_addr = MAX(max_addr, end);

    if ((p->Flags & ELF_PF_X) && eh->Entry >= address &&
        eh->Entry < address + p->MemSize)
      executable_entry = TRUE;
  }

  if (min_addr == ~(UINT64)0 || max_addr <= min_addr || !executable_entry)
    return EFI_INVALID_PARAMETER;

  UINTN pages = (UINTN)((max_addr - min_addr) / PAGE_SIZE);
  EFI_PHYSICAL_ADDRESS allocation = min_addr;

  EFI_STATUS status =
      bs->AllocatePages(AllocateAddress, EfiLoaderData, pages, &allocation);

  if (EFI_ERROR(status))
    return status;

  bs->SetMem((VOID *)(UINTN)allocation, (UINTN)(max_addr - min_addr), 0);

  for (UINTN i = 0; i < eh->PhNum; i++) {
    ELF64_PHDR *p = &phdrs[i];

    if (p->Type != ELF_PT_LOAD || p->MemSize == 0)
      continue;

    UINT64 address = p->PAddr ? p->PAddr : p->VAddr;

    bs->CopyMem((VOID *)(UINTN)address, image + p->Offset, (UINTN)p->FileSize);
  }

  *entry = eh->Entry;
  return EFI_SUCCESS;
}

EFI_STATUS EFIAPI efi_main(EFI_HANDLE ImageHandle,
                           EFI_SYSTEM_TABLE *SystemTable) {
  EFI_STATUS status;
  EFI_BOOT_SERVICES *bs = SystemTable->BootServices;

  EFI_HANDLE device;
  LOADED_IMAGE *loaded;
  EFI_SIMPLE_FILE_SYSTEM_PROTOCOL *fs;
  EFI_FILE_PROTOCOL *root;
  EFI_FILE_PROTOCOL *file;

  VOID *info_buffer = NULL;
  VOID *image_buffer = NULL;
  VOID *memory_map = NULL;
  BOOT_INFO *boot_info = NULL;

  UINT64 kernel_entry = 0;

  SystemTable->ConOut->OutputString(SystemTable->ConOut,
                                    L"OPL: Bootloader started\r\n");

  // 1. Get loaded image protocol.

  status = bs->HandleProtocol(ImageHandle, &LoadedImageGuid, (VOID **)&loaded);

  if (EFI_ERROR(status))
    goto error;

  device = loaded->DeviceHandle;

  // 2. Get filesystem from boot device.

  status = bs->HandleProtocol(device, &FileSystemGuid, (VOID **)&fs);

  if (EFI_ERROR(status))
    goto error;

  status = fs->OpenVolume(fs, &root);

  if (EFI_ERROR(status))
    goto error;

  // 3. Open kernel.

  status = root->Open(root, &file, L"\\EFI\\OPL\\kernel.elf", EFI_FILE_MODE_READ, 0);


  if (EFI_ERROR(status))
    goto error;

  // 4. Get kernel file size.

  UINTN info_size = 0;

  status = file->GetInfo(file, &FileInfoGuid, &info_size, NULL);
  if (status != EFI_BUFFER_TOO_SMALL)
    goto error;


  status = bs->AllocatePool(EfiLoaderData, info_size, &info_buffer);

  if (EFI_ERROR(status))
    goto error;

  status = file->GetInfo(file, &FileInfoGuid, &info_size, info_buffer);

  if (EFI_ERROR(status))
    goto error;

  EFI_FILE_INFO *info = (EFI_FILE_INFO *)info_buffer;

  if (info->FileSize == 0 || info->FileSize > (UINT64)(~(UINTN)0)) {
    status = EFI_INVALID_PARAMETER;
    goto error;
  }

  UINTN file_size = (UINTN)info->FileSize;

  // 5. Read entire ELF file.

  status = bs->AllocatePool(EfiLoaderData, file_size, &image_buffer);

  if (EFI_ERROR(status))
    goto error;

  UINTN read_size = file_size;

  status = file->Read(file, &read_size, image_buffer);

  if (EFI_ERROR(status) || read_size != file_size)
    goto error;

  file->Close(file);
  root->Close(root);

  bs->FreePool(info_buffer);
  info_buffer = NULL;

  SystemTable->ConOut->OutputString(SystemTable->ConOut,
                                    L"OPL: Kernel loaded into buffer\r\n");

  // 6. Parse ELF and map PT_LOAD segments.

  status = load_elf(bs, (UINT8 *)image_buffer, file_size, &kernel_entry);

  if (EFI_ERROR(status))
    goto error;

  bs->FreePool(image_buffer);
  image_buffer = NULL;

  SystemTable->ConOut->OutputString(SystemTable->ConOut,
                                    L"OPL: ELF segments mapped\r\n");

  // 7. Allocate boot information.

  status =
      bs->AllocatePool(EfiLoaderData, sizeof(BOOT_INFO), (VOID **)&boot_info);

  if (EFI_ERROR(status))
    goto error;

  boot_info->magic = OPL_BOOT_MAGIC;
  boot_info->version = OPL_BOOT_VERSION;
  boot_info->size = sizeof(BOOT_INFO);
  boot_info->mmap = 0;
  boot_info->mmap_size = 0;
  boot_info->mmap_desc_size = 0;
  boot_info->mmap_desc_version = 0;
  boot_info->_pad0 = 0;
  boot_info->fb_base = boot_info->fb_size = 0;
  boot_info->fb_width = boot_info->fb_height = boot_info->fb_pitch = 0;
  boot_info->fb_format = OPL_FB_NONE;
  boot_info->acpi_rsdp = 0;
  boot_info->smbios = 0;
  boot_info->smbios_v3 = 0;
  boot_info->_pad1 = 0;

  // Framebuffer (GOP) — optional.
  {
    EFI_GRAPHICS_OUTPUT_PROTOCOL *gop = NULL;
    if (!EFI_ERROR(bs->LocateProtocol(&GopGuid, NULL, (VOID **)&gop)) && gop &&
        gop->Mode && gop->Mode->Info) {
      EFI_GRAPHICS_OUTPUT_MODE_INFORMATION *mi = gop->Mode->Info;
      UINT32 fmt = OPL_FB_NONE;
      if (mi->PixelFormat == PixelRedGreenBlueReserved8BitPerColor)
        fmt = OPL_FB_RGBX;
      else if (mi->PixelFormat == PixelBlueGreenRedReserved8BitPerColor)
        fmt = OPL_FB_BGRX;
      if (fmt != OPL_FB_NONE) {
        boot_info->fb_base = gop->Mode->FrameBufferBase;
        boot_info->fb_size = gop->Mode->FrameBufferSize;
        boot_info->fb_width = mi->HorizontalResolution;
        boot_info->fb_height = mi->VerticalResolution;
        boot_info->fb_pitch = mi->PixelsPerScanLine * 4;
        boot_info->fb_format = fmt;
      }
    }
  }

  // ACPI RSDP from the configuration table.
  for (UINTN i = 0; i < SystemTable->NumberOfTableEntries; i++) {
    if (guid_eq(&SystemTable->ConfigurationTable[i].VendorGuid, &Acpi20Guid)) {
      boot_info->acpi_rsdp = (UINT64)(UINTN)SystemTable->ConfigurationTable[i].VendorTable;
    } else if (guid_eq(&SystemTable->ConfigurationTable[i].VendorGuid, &Smbios3Guid)) {
      boot_info->smbios = (UINT64)(UINTN)SystemTable->ConfigurationTable[i].VendorTable;
      boot_info->smbios_v3 = 1;
    } else if (!boot_info->smbios &&
               guid_eq(&SystemTable->ConfigurationTable[i].VendorGuid, &SmbiosGuid)) {
      boot_info->smbios = (UINT64)(UINTN)SystemTable->ConfigurationTable[i].VendorTable;
    }
  }

  // 8. Prepare memory map buffer.

  UINTN map_size = 0;
  UINTN map_key = 0;
  UINTN descriptor_size = 0;
  UINT32 descriptor_version = 0;

  status = bs->GetMemoryMap(&map_size, NULL, &map_key, &descriptor_size,
                            &descriptor_version);

  if (status != EFI_BUFFER_TOO_SMALL)
    goto error;

  map_size += (descriptor_size ? descriptor_size * 8 : 4096);

  status = bs->AllocatePool(EfiLoaderData, map_size, &memory_map);

  if (EFI_ERROR(status))
    goto error;

  // 9. Get memory map and ExitBootServices.
  // No allocations between GetMemoryMap and ExitBootServices.

  for (UINTN attempt = 0; attempt < 4; attempt++) {
    UINTN current_size = map_size;

    status =
        bs->GetMemoryMap(&current_size, (EFI_MEMORY_DESCRIPTOR *)memory_map,
                         &map_key, &descriptor_size, &descriptor_version);

    if (status == EFI_BUFFER_TOO_SMALL) {
      bs->FreePool(memory_map);

      map_size = current_size + descriptor_size * 8;

      status = bs->AllocatePool(EfiLoaderData, map_size, &memory_map);

      if (EFI_ERROR(status))
        goto error;

      continue;
    }

    if (EFI_ERROR(status))
      goto error;

    boot_info->mmap = (UINT64)(UINTN)memory_map;
    boot_info->mmap_size = current_size;
    boot_info->mmap_desc_size = descriptor_size;
    boot_info->mmap_desc_version = descriptor_version;

    status = bs->ExitBootServices(ImageHandle, map_key);

    if (!EFI_ERROR(status))
      break;

    if (status != EFI_INVALID_PARAMETER)
      goto error;

    if (attempt == 3)
      goto error;
  }

  // 10. Kernel entry. No UEFI calls beyond this point.

  KERNEL_ENTRY entry = (KERNEL_ENTRY)(UINTN)kernel_entry;

  entry(boot_info);

  for (;;)
    __asm__ volatile("pause");

error:

  SystemTable->ConOut->OutputString(SystemTable->ConOut,
                                    L"OPL: Bootloader failed\r\n");

  return status;
}

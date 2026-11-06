kernel-project-root-directory/
├── boot/
│   ├── bootloader/
│   ├── entry/
│   └── startup/
│
├── arch/
│   ├── x86_64/
│   │   ├── cpu/
│   │   ├── interrupts/
│   │   ├── paging/
│   │   ├── context/
│   │   └── syscall/
│   └── aarch64/
│
├── hal/
│   ├── cpu/
│   ├── memory/
│   ├── interrupt/
│   ├── timer/
│   ├── dma/
│   └── pci/
│
├── kernel/
│   ├── core/
│   ├── process/
│   ├── thread/
│   ├── scheduler/
│   ├── synchronization/
│   ├── ipc/
│   ├── syscall/
│   └── panic/
│
├── mm/
│   ├── physical/
│   ├── virtual/
│   ├── heap/
│   ├── slab/
│   └── address_space/
│
├── drivers/
│   ├── bus/
│   ├── storage/
│   ├── network/
│   ├── input/
│   ├── display/
│   ├── gpu/
│   └── audio/
│
├── subsystems/
│   ├── graphics/
│   ├── networking/
│   ├── filesystem/
│   ├── security/
│   └── power/
│
├── runtime/
│   ├── memory/
│   ├── constructors/
│   ├── strings/
│   ├── containers/
│   └── synchronization/
│
├── include/
│   └── kernel/
│       ├── interfaces/
│       ├── types/
│       └── contracts/
│
├── tests/
│   ├── unit/
│   ├── integration/
│   └── hardware/
│
├── tools/
├── linker/
├── scripts/
├── docs/
└── build/
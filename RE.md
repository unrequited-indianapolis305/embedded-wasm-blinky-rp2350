# Reverse Engineering: embedded-wasm-blinky

## Table of Contents

1. [Binary Overview](#1-binary-overview)
2. [ELF Header](#2-elf-header)
3. [Section Layout](#3-section-layout)
4. [Memory Map & Segments](#4-memory-map--segments)
5. [Boot Sequence](#5-boot-sequence)
6. [Vector Table](#6-vector-table)
7. [Firmware Function Map](#7-firmware-function-map)
8. [Hardware Register Access](#8-hardware-register-access)
9. [Pulley Interpreter Deep Dive](#9-pulley-interpreter-deep-dive)
10. [Embedded cwasm Blob](#10-embedded-cwasm-blob)
11. [Host-Guest Call Flow](#11-host-guest-call-flow)
12. [RE Observations](#12-re-observations)
13. [Pulley Instruction Set Architecture](#13-pulley-instruction-set-architecture)
14. [Pulley Bytecode Disassembly](#14-pulley-bytecode-disassembly)
15. [Ghidra Analysis Walkthrough](#15-ghidra-analysis-walkthrough)

---

## 1. Binary Overview

| Property          | Value                                    |
| ----------------- | ---------------------------------------- |
| File              | `embedded-wasm-blinky`                   |
| Size on disk      | 1,174,232 bytes (1.12 MiB)               |
| Format            | ELF32 ARM little-endian                  |
| ABI               | EABI5, hard-float                        |
| Target            | ARMv8-M Mainline (Cortex-M33)            |
| MCU               | RP2350 (Raspberry Pi Pico 2)             |
| Stripped          | No (symbol table + string table present) |
| Text functions    | 2,375                                    |
| Read-only symbols | 2,660                                    |

The binary is a bare-metal `no_std` Rust firmware that hosts a Wasmtime
Component Model runtime with the Pulley bytecode interpreter. It blinks
the onboard LED on GPIO25 at 500 ms intervals, with the blink logic
running as a WebAssembly guest component.

---

## 2. ELF Header

```
Magic:   7f 45 4c 46 01 01 01 03 00 00 00 00 00 00 00 00
Class:                             ELF32
Data:                              2's complement, little endian
Version:                           1 (current)
OS/ABI:                            UNIX - GNU
Type:                              EXEC (Executable file)
Machine:                           ARM
Entry point address:               0x1000010d
Flags:                             0x5000400, Version5 EABI, hard-float ABI
Program headers:                   6 (at offset 52)
Section headers:                   16 (at offset 1,173,592)
```

**Entry Point**: `0x1000010d` — the `Reset` handler in `.text`. The LSB is
set (0x0D vs 0x0C) to indicate Thumb mode, required by ARMv8-M. The actual
code starts at `0x1000010c`.

---

## 3. Section Layout

```
Nr  Name            Type        Addr        Size      Flags  Description
 1  .vector_table   PROGBITS    0x10000000  0x000f8   A      ARM exception + interrupt vectors
 2  .start_block    PROGBITS    0x100000f8  0x00014   AR     RP2350 IMAGE_DEF boot metadata
 3  .text           PROGBITS    0x1000010c  0x84c70   AX     All executable code (532 KiB)
 4  .bi_entries     PROGBITS    0x10084d7c  0x00000   A      Binary info entries (empty)
 5  .rodata         PROGBITS    0x10084d80  0x1dbb0   AMSR   Read-only data (120 KiB)
 6  .data           PROGBITS    0x20000000  0x00024   WA     Initialized globals (36 bytes)
 7  .gnu.sgstubs    PROGBITS    0x100a2960  0x00000   A      Secure gateway stubs (empty)
 8  .bss            NOBITS      0x20000028  0x400a4   WA     Zero-init data (256 KiB)
 9  .uninit         NOBITS      0x200400cc  0x00000   WA     Uninitialized memory (empty)
10  .end_block      PROGBITS    0x100a2960  0x00000   WA     Block end marker (empty)
13  .symtab         SYMTAB      file only   0x1dea0          Symbol table (7,702 entries)
15  .strtab         STRTAB      file only   0x5dda0          String table (383 KiB)
```

### Size Breakdown

The Pico 2 board has **4 MiB** (4,194,304 B) of external QSPI flash.
The linker script (`rp2350.x`) conservatively allocates **2 MiB**
(2,097,152 B). Percentages below are relative to the 4 MiB physical
flash.

| Region         | Section         | Size          | % of 4 MiB Flash     |
| -------------- | --------------- | ------------- | -------------------- |
| Code           | `.text`         | 544,880 B     | 13.0%                |
| Constants      | `.rodata`       | 122,800 B     | 2.9%                 |
| Vectors        | `.vector_table` | 248 B         | <0.1%                |
| Boot meta      | `.start_block`  | 20 B          | <0.1%                |
| Init data      | `.data`         | 36 B          | <0.1%                |
| **Flash used** |                 | **667,984 B** | **15.9%**            |
| **Flash free** |                 | 3,526,320 B   | 84.1%                |
| BSS (RAM)      | `.bss`          | 262,308 B     | 51.2% of 512 KiB RAM |

The firmware uses 652 KiB of the available 4 MiB flash (31.9% of the
2 MiB linker allocation). The `.text` section (532 KiB) is dominated by
the Wasmtime runtime: the Pulley interpreter dispatch loop, serde
deserialization for component metadata, BTree collections, type registry,
and component linker code. The actual blinky firmware functions occupy
only ~2 KiB.

---

## 4. Memory Map & Segments

```
Segment  VirtAddr     PhysAddr     MemSiz   Flags  Contents
  0      0x10000000   0x10000000   0x0010c  R      .vector_table + .start_block
  1      0x1000010c   0x1000010c   0x84c70  R E    .text (executable code)
  2      0x10084d7c   0x10084d7c   0x1dbb4  R      .rodata (constants + cwasm blob)
  3      0x20000000   0x100a2930   0x00024  RW     .data (LMA in flash, VMA in RAM)
  4      0x20000028   0x20000028   0x400a4  RW     .bss (zero-filled at boot)
  5      0x00000000   0x00000000   0x00000  RW     GNU_STACK (zero-size)
```

### Physical Address Space

```
Flash (XIP):  0x10000000 - 0x100a2953  (668 KiB used of 4 MiB)
              +-- 0x10000000  Vector table (248 B)
              +-- 0x100000f8  IMAGE_DEF boot block (20 B)
              +-- 0x1000010c  .text starts (Reset handler)
              +-- 0x10084d80  .rodata starts
              +-- 0x1008a059  Embedded cwasm (Pulley ELF, ~24 KiB)
              +-- 0x100a2930  .data initializers (36 B, copied to RAM)

RAM (SRAM):   0x20000000 - 0x200400cb  (256 KiB used of 512 KiB)
              +-- 0x20000000  .data (36 B: UART state, TLS value)
              +-- 0x20000028  TLS_VALUE (4 B)
              +-- 0x2000002c  led::PINS (16 B)
              +-- 0x2000003c  HEAP_MEM (262,144 B = 256 KiB)
              +-- 0x2004003c  HEAP allocator struct (32 B)

Stack:        0x20080000  Initial SP (top of 512 KiB SRAM, grows down)
```

**Segment 3** has a split-address load: `PhysAddr = 0x100a2930` (flash)
but `VirtAddr = 0x20000000` (RAM). The Reset handler copies these 36
bytes from flash to RAM during the `.data` initialization loop.

---

## 5. Boot Sequence

### 5.1 RP2350 Boot ROM -> IMAGE_DEF

The RP2350 Boot ROM scans flash for a valid image definition block. Our
`.start_block` section at `0x100000f8` contains:

```
d3deffff 42012110 ff010000 00000000 793512ab
```

This is `hal::block::ImageDef::secure_exe()` — it tells the Boot ROM
this is a secure ARM executable, allowing the ROM to set up XIP and
transfer control to the vector table.

### 5.2 Vector Table -> Reset Handler

The processor reads the vector table at `0x10000000`:

```
Word 0: 0x20080000  <- Initial Stack Pointer (top of 512 KiB SRAM)
Word 1: 0x1000010d  <- Reset vector (Thumb-mode address of Reset handler)
```

After power-on reset, the CPU loads SP from word 0 and branches to the
Reset vector.

### 5.3 Reset Handler (0x1000010c)

```armasm
Reset:
    bl      DefaultPreInit          ; No-op (empty pre-init hook)

    ; --- Zero .bss (0x20000028 -> 0x200400cc) ---
    ldr     r0, =0x20000028         ; BSS start
    ldr     r1, =0x200400cc         ; BSS end
    movs    r2, #0
.bss_loop:
    cmp     r1, r0
    beq     .bss_done
    stmia   r0!, {r2}              ; Write zero, advance pointer
    b       .bss_loop

    ; --- Copy .data from flash to RAM ---
.bss_done:
    ldr     r0, =0x20000000         ; RAM dest (.data VMA)
    ldr     r1, =0x20000024         ; RAM end
    ldr     r2, =0x100a2930         ; Flash source (.data LMA)
.data_loop:
    cmp     r1, r0
    beq     .data_done
    ldmia   r2!, {r3}
    stmia   r0!, {r3}
    b       .data_loop

    ; --- Enable FPU ---
.data_done:
    ldr     r0, =0xe000ed88         ; CPACR (Coprocessor Access Control)
    ldr     r2, [r0]
    orr     r2, r2, #0xf00000      ; Full access to CP10 + CP11 (FPU)
    str     r2, [r0]
    dsb     sy
    isb     sy

    bl      main                    ; Enter Rust main()
    udf     #0                      ; Trap if main returns (should never)
```

### 5.4 `main()` (0x10008410)

The `#[hal::entry]` macro generates a thin wrapper that calls
`__cortex_m_rt_main` at `0x100075a8`:

```armasm
main:
    ; Enable all 32 SIO GPIO outputs
    ldr     r0, =0xd0000100         ; SIO GPIO_OE base
    movs    r1, #1
    str     r1, [r0, #0]            ; GPIO_OE_SET for each bank
    ...

    ; Enable FPU (redundant, also done in Reset — belt & suspenders)
    ldr     r0, =0xe000ed88         ; CPACR
    ldr     r1, [r0]
    orr     r1, r1, #0x303          ; CP10 + CP11 full access
    str     r1, [r0]

    bl      init_heap               ; Initialize 256 KiB heap allocator
    bl      init_hardware           ; Clock setup, UART, GPIO
    bl      run_wasm                ; Create Wasmtime engine, run component
```

---

## 6. Vector Table

The vector table at `0x10000000` is 248 bytes (62 entries):

```
Offset  Vector              Handler          Address
0x0000  Initial SP          —                0x20080000
0x0004  Reset               Reset            0x1000010d
0x0008  NMI                 DefaultHandler   0x1007c7ed
0x000c  HardFault           HardFault_       0x10084d75
0x0010  MemManage           DefaultHandler   0x1007c7ed
0x0014  BusFault            DefaultHandler   0x1007c7ed
0x0018  UsageFault          DefaultHandler   0x1007c7ed
0x001c  SecureFault         DefaultHandler   0x1007c7ed
0x0020  Reserved            —                0x00000000
0x0024  Reserved            —                0x00000000
0x0028  Reserved            —                0x00000000
0x002c  SVCall              DefaultHandler   0x1007c7ed
0x0030  DebugMonitor        DefaultHandler   0x1007c7ed
0x0034  Reserved            —                0x00000000
0x0038  PendSV              DefaultHandler   0x1007c7ed
0x003c  SysTick             DefaultHandler   0x1007c7ed
0x0040+ IRQ0-IRQ51          DefaultHandler   0x1007c7ed
```

### DefaultHandler (0x1007c7ec)

```armasm
DefaultHandler_:
    push    {r7, lr}
    mov     r7, sp
    b.n     .                       ; Infinite loop (hang on unhandled exception)
```

### HardFault (0x10084d74)

```armasm
HardFault_:
    push    {r7, lr}
    mov     r7, sp
    b.n     .                       ; Infinite loop (hang on hard fault)
```

Both handlers preserve the frame pointer then spin forever. This is the
`cortex-m-rt` default — no recovery, no diagnostic output.

No peripheral interrupts are used. All 52 IRQ vectors point to
`DefaultHandler`. The firmware is entirely polled:

- GPIO writes are direct SIO register stores
- UART TX is blocking (poll TXFF flag)
- Delays use SysTick polling via `cortex_m::delay::Delay`

---

## 7. Firmware Function Map

### 7.1 Application Functions

| Address      | Size  | Symbol               | Purpose                                                                   |
| ------------ | ----- | -------------------- | ------------------------------------------------------------------------- |
| `0x1000010c` | 0x3e  | `Reset`              | BSS zero, .data copy, FPU enable, call main                               |
| `0x100073dc` | 0x1cc | `init_hardware`      | Clocks, UART0, GPIO25, SysTick                                            |
| `0x100075a8` | 0x6c  | `__cortex_m_rt_main` | SIO GPIO enable, FPU, init_heap, init_hardware, run_wasm                  |
| `0x10007760` | 0x856 | `run_wasm`           | Engine::new, Component::deserialize, Store::new, Linker, instantiate, run |
| `0x10007fb8` | 0x20  | `init_heap`          | Initialize 256 KiB linked-list heap                                       |
| `0x10006858` | 0xc2  | `led::set_low`       | Clear GPIO via SIO, UART log                                              |
| `0x1000691c` | 0xc2  | `led::set_high`      | Set GPIO via SIO, UART log                                                |
| `0x100069e0` | 0x108 | `led::store_pin`     | Store GPIO pin handle for led module                                      |
| `0x10007614` | 0x14c | `uart::write_msg`    | Blocking UART TX                                                          |
| `0x10008410` | 0x8   | `main`               | Thin #[entry] wrapper                                                     |
| `0x1007c7ec` | 0x6   | `DefaultHandler`     | Infinite loop (unhandled exception)                                       |
| `0x1007c7f4` | 0x6   | `DefaultPreInit`     | No-op (returns immediately)                                               |
| `0x10084d74` | 0x6   | `HardFault`          | Infinite loop (hard fault)                                                |

### 7.2 Wasmtime Runtime (Top-20 by Size)

| Address      | Size     | Demangled Name                                                 |
| ------------ | -------- | -------------------------------------------------------------- |
| `0x10032c98` | 16,464 B | `wasmtime_environ::tunables::OperatorCost::deserialize`        |
| `0x10068210` | 16,456 B | `pulley_interpreter::decode::decode_one_extended`              |
| `0x10064ef4` | 12,518 B | `pulley_interpreter::interp::Interpreter::run` (dispatch loop) |
| `0x1002e43c` | 8,696 B  | `wasmtime::engine::serialization::Metadata::check_cost`        |
| `0x1000d7b8` | 2,304 B  | `wasmtime::runtime::vm::interpreter::InterpreterRef::call`     |

### 7.3 BSS Layout

| Address      | Size      | Symbol      | Purpose                         |
| ------------ | --------- | ----------- | ------------------------------- |
| `0x20000028` | 4 B       | `TLS_VALUE` | Wasmtime TLS shim (platform.rs) |
| `0x2000002c` | 16 B      | `led::PINS` | GPIO pin handles for LED module |
| `0x2000003c` | 262,144 B | `HEAP_MEM`  | Raw heap backing memory         |
| `0x2004003c` | 32 B      | `HEAP`      | Linked-list allocator state     |

### 7.4 Data Layout

| Address      | Size | Symbol          | Purpose                      |
| ------------ | ---- | --------------- | ---------------------------- |
| `0x20000000` | 1 B  | `UART` (.1)     | UART initialized flag        |
| `0x20000001` | 35 B | (padding/other) | Remaining .data initializers |

---

## 8. Hardware Register Access

### 8.1 Peripheral Base Addresses

| Base Address | Peripheral | Usage in Firmware       |
| ------------ | ---------- | ----------------------- |
| `0x40020000` | RESETS     | Subsystem reset control |
| `0x40028000` | IO_BANK0   | GPIO function selection |
| `0x40030000` | PADS_BANK0 | Pad configuration       |
| `0x40040000` | XOSC       | Crystal oscillator      |
| `0x40048000` | PLL_SYS    | System PLL (150 MHz)    |
| `0x4004c000` | PLL_USB    | USB PLL (48 MHz)        |
| `0x40050000` | CLOCKS     | Clock generators        |
| `0x40070000` | UART0      | Debug serial output     |
| `0xd0000000` | SIO        | Single-cycle I/O (GPIO) |
| `0xe000e010` | SysTick    | System timer (delay)    |
| `0xe000ed88` | CPACR      | FPU access control      |

### 8.2 GPIO Control (LED Blink)

The LED module writes directly to the SIO (Single-cycle I/O) registers
to toggle GPIO25:

```
led::set_high (0x1000691c):
    ; SIO GPIO output set register
    ldr     r0, =0xd0000014         ; SIO GPIO_OUT_SET
    movs    r1, #1
    lsls    r1, r1, #25             ; Bit 25 (GPIO25)
    str     r1, [r0]                ; Set GPIO25 high

led::set_low (0x10006858):
    ; SIO GPIO output clear register
    ldr     r0, =0xd0000018         ; SIO GPIO_OUT_CLR
    movs    r1, #1
    lsls    r1, r1, #25             ; Bit 25 (GPIO25)
    str     r1, [r0]                ; Set GPIO25 low
```

GPIO25 is configured as a push-pull SIO output during `init_hardware`:

```
init_hardware (0x100073dc):
    ; Configure GPIO25 pad
    movw    r0, #0x806c
    movt    r0, #0x4003             ; 0x4003806c = PADS_BANK0 GPIO25
    ldr     r1, [r0]
    bfi     r1, r2, #6, #3         ; Output enable, drive strength
    str     r1, [r0]

    ; Set GPIO25 function to SIO (function 5)
    movw    r0, #0x80cc
    movt    r0, #0x4002             ; 0x400280cc = IO_BANK0 GPIO25_CTRL
    movs    r1, #5                  ; Function 5 = SIO
    str     r1, [r0]
```

### 8.3 SysTick Timer (Delay)

```
init_hardware:
    ; Configure SysTick for processor clock
    movw    r1, #0xe010
    movt    r1, #0xe000             ; 0xe000e010 = SYST_CSR
    ldr     r0, [r1]
    orr     r2, r0, #4             ; Set CLKSOURCE (processor clock)
    str     r2, [r1]
```

### 8.4 UART0 Configuration

```
uart::init:
    ; GPIO0 -> UART0 TX (function 2)
    ; GPIO1 -> UART0 RX (function 2)
    ; Baud rate 115200 at 150 MHz:
    ;   IBRD = 81, FBRD = 24
    ; UART0 base = 0x40070000
```

---

## 9. Pulley Interpreter Deep Dive

### 9.1 Architecture

Pulley is Wasmtime's portable bytecode interpreter. Instead of emitting
native ARM code (which would require a JIT and MMU), the build script
AOT-compiles the Wasm component to Pulley bytecode during `cargo build`.
At runtime, the ARM firmware interprets this bytecode instruction by
instruction.

### 9.2 Interpreter Entry (`InterpreterRef::call`)

```
Location:  0x1000d7b8  (2,304 bytes)
```

This function is the bridge between native ARM code and the Pulley VM.
Call sequence:

```
run_wasm()
  -> Blinky::instantiate()
    -> Blinky::call_run()
      -> InterpreterRef::call()     <-- native-to-Pulley boundary
        -> Vm::call_start()         ; Set up Pulley register file
        -> Vm::call_run()           ; Enter interpreter loop
          -> Interpreter::run()     ; Main dispatch loop
```

### 9.3 Main Dispatch Loop (`Interpreter::run`)

```
Location:  0x10064ef4  (12,518 bytes)
```

This is the hot loop that executes Pulley bytecode. It uses a two-level
dispatch scheme:

**Level 1: Primary opcode (1-byte)**

```armasm
Interpreter::run (0x10064ef4):
    push    {r4-r7, lr}
    stmdb   sp!, {r8-r11}
    sub     sp, #12
    mov.w   r8, #0x80000000         ; Sentinel value

.fetch:
    mov     r0, r1                  ; r1 = program counter (PC)
    ldrb.w  r2, [r0], #1           ; Fetch 1-byte opcode, advance PC
    str     r0, [sp, #8]            ; Save updated PC

    tbh     [pc, r2, lsl #1]        ; Table Branch Halfword — 256-entry jump table
```

The `tbh` instruction is a Thumb-2 table branch. It reads a halfword
from `pc + r2*2` and branches to `pc + offset*2`. This gives a direct
computed goto to 256 possible opcode handlers.

**Level 2: Extended opcode (2-byte prefix)**

When the primary opcode is the "extended" sentinel (0xDC), a second
2-byte opcode is fetched:

```
Location:  0x10068210  (16,456 bytes)
```

### 9.4 Register File

The Pulley VM uses a virtual register file stored in the `Vm` struct:

- **x0-x30**: 64-bit general-purpose registers (at offset `+0x200`,
  8 bytes each)
- **f0-f31**: 128-bit float/vector registers
- **sp**: stack pointer (at offset `+0x40c` from Vm base)
- **lr**: return address register

### 9.5 Host Call Mechanism

When Pulley bytecode calls a host-imported function (like
`gpio::set_high` or `timing::delay_ms`), the interpreter returns with
code 0 (`CallIndirectHost`). The `InterpreterRef::call` loop:

1. Reads the host function index from the Pulley state
2. Looks up the host function in the trap handler TLS
3. Calls the native ARM host function directly
4. Stores the return value back into Pulley registers
5. Re-enters `Vm::call_run` to continue interpreting

---

## 10. Embedded cwasm Blob

### 10.1 Location and Format

The precompiled Pulley bytecode is embedded in `.rodata` at
`0x1008a059`, referenced by the `WASM_BINARY` constant. It is
**24,680 bytes** (0x6068).

The blob is itself an **ELF file** — Wasmtime's serialization format
wraps Pulley bytecode in a standard ELF container:

```
Hex dump at 0x1008a059:
7f 45 4c 46 02 01 01 c8 ... .ELF....
```

Header analysis:

| Field  | Value                          |
| ------ | ------------------------------ |
| Magic  | `\x7fELF`                      |
| Class  | ELF64 (byte `02` at offset 4)  |
| Data   | Little-endian                  |
| Target | `pulley32-unknown-unknown-elf` |

### 10.2 Build Pipeline

The cwasm is produced by the build script (`build.rs`):

```
wasm-app/src/lib.rs
    |
    v  (cargo build --target wasm32-unknown-unknown)
wasm_app.wasm  (core Wasm module with wit-bindgen metadata)
    |
    v  (ComponentEncoder::encode())
component.wasm  (Wasm Component Model binary)
    |
    v  (engine.precompile_component())
blinky.cwasm  (Pulley ELF container with pre-lowered bytecode)
    |
    v  (include_bytes! in firmware)
WASM_BINARY const in .rodata
```

At runtime, `Component::deserialize()` reads this blob, validates the
engine configuration matches (target, tunables, memory settings), and
maps the Pulley bytecode sections for the interpreter.

### 10.3 Guest Code Logic

The guest component (`wasm-app/src/lib.rs`) compiles down to Pulley
bytecode that implements:

```
const LED_PIN: u32 = 25;
loop {
    call_import gpio::set_high(LED_PIN)    -> host ARM code
    call_import timing::delay_ms(500)      -> host ARM code
    call_import gpio::set_low(LED_PIN)     -> host ARM code
    call_import timing::delay_ms(500)      -> host ARM code
}
```

Each `call_import` triggers the `CallIndirectHost` return path in the
Pulley interpreter, transitioning from bytecode execution to native ARM
host functions.

---

## 11. Host-Guest Call Flow

### 11.1 Full Call Chain for `gpio::set_high(25)`

```
[Pulley VM]  Bytecode: call_indirect_host #0, args=[25]
    |
    v  (Interpreter::run returns CallIndirectHost)
[ARM Native]  InterpreterRef::call (0x1000d7b8)
    |  reads host function index, dispatches
    v
[ARM Native]  <HostState as gpio::Host>::set_high
    |
    v
[ARM Native]  led::set_high (0x1000691c)
    |
    v
[Hardware]  SIO GPIO_OUT_SET @ 0xd0000014
            Bit 25 set = GPIO25 driven high (LED on)
```

### 11.2 Full Call Chain for `timing::delay_ms(500)`

```
[Pulley VM]  Bytecode: call_indirect_host #2, args=[500]
    |
    v  (Interpreter::run returns CallIndirectHost)
[ARM Native]  InterpreterRef::call (0x1000d7b8)
    |
    v
[ARM Native]  <HostState as timing::Host>::delay_ms
    |
    v
[Hardware]  SysTick countdown for 500 ms (polling loop)
```

### 11.3 UART Diagnostic Output

After setting or clearing the GPIO, the host function writes a
diagnostic message via UART0:

```
led::set_high:
    bl      uart::write_msg         ; Print "GPIO 25 High\n"

led::set_low:
    bl      uart::write_msg         ; Print "GPIO 25 Low\n"
```

---

## 12. RE Observations

### 12.1 Binary Composition

The firmware is ~99.6% Wasmtime runtime, ~0.4% application code:

| Component                   | Approx Size | % of .text |
| --------------------------- | ----------- | ---------- |
| Wasmtime runtime            | ~542 KiB    | 99.4%      |
| Pulley interpreter (run)    | 12.2 KiB    | 2.3%       |
| Pulley decoder (extended)   | 16.1 KiB    | 3.0%       |
| Serde deserialization       | ~50 KiB     | 9.2%       |
| Application (led+uart+main) | ~2.5 KiB    | 0.5%       |

### 12.2 What a Reverse Engineer Would See in Ghidra

Opening this binary in Ghidra with the ARM Cortex processor module:

1. **Immediate recognition**: ELF with symbols — Ghidra will auto-analyze
   and name ~7,700 functions. Rust mangled names are long but readable.

2. **Vector table**: Ghidra's ARM analyzer will find the vector table at
   `0x10000000` and identify the Reset handler.

3. **The `tbh` dispatch**: The Pulley interpreter's `tbh` (Table Branch
   Halfword) at `0x10064f18` creates a 256-entry switch statement. Ghidra
   will identify this as a switch/case and create labeled branches, but
   the sheer size (12 KiB of handlers) makes manual analysis tedious.

4. **No obfuscation**: The binary is not stripped, not packed, and has
   full symbol tables. All function boundaries are recoverable.

5. **The cwasm blob**: A reverse engineer would find the `\x7fELF` magic
   inside `.rodata` and recognize it as an embedded ELF. Extracting and
   analyzing it separately reveals Pulley bytecode — an unfamiliar ISA
   that requires understanding the Pulley opcode table to disassemble.

6. **Hardware identification**: The peripheral base addresses
   (`0xd0000014` SIO, `0x40070000` UART0) immediately identify this as
   an RP2350/RP2040-family device.

### 12.3 Security Observations

- **No code signing**: The ELF is loaded as a plain binary, no signature
  verification at the Wasmtime level (RP2350 Secure Boot is separate).

- **Full symbol table**: 383 KiB of debug strings reveal internal
  structure, function names, and crate dependencies. A production build
  should strip symbols (`cargo build --release` + `strip`).

- **Deterministic execution**: No interrupts, no DMA — the firmware is
  entirely sequential and predictable, which simplifies both RE and
  timing analysis.

- **Wasmtime sandboxing**: The Wasm guest cannot access hardware directly;
  all I/O goes through the host's `set_high`, `set_low`, and `delay_ms`
  functions, which bounds the guest's capability to LED control and
  timing.

### 12.4 Key Addresses Quick Reference

| Address      | What                                          |
| ------------ | --------------------------------------------- |
| `0x10000000` | Vector table (initial SP + exception vectors) |
| `0x100000f8` | RP2350 IMAGE_DEF boot block                   |
| `0x1000010c` | Reset handler (entry point)                   |
| `0x100073dc` | init_hardware                                 |
| `0x100075a8` | __cortex_m_rt_main                            |
| `0x10007760` | run_wasm                                      |
| `0x10007fb8` | init_heap                                     |
| `0x1000691c` | led::set_high (host binding)                  |
| `0x10006858` | led::set_low (host binding)                   |
| `0x10008410` | main (thin wrapper)                           |
| `0x10007614` | uart::write_msg                               |
| `0x10064ef4` | Pulley Interpreter::run (dispatch loop)       |
| `0x10068210` | Pulley decode_one_extended                    |
| `0x1000d7b8` | InterpreterRef::call (native->Pulley bridge)  |
| `0x1008a059` | Embedded cwasm blob (Pulley ELF)              |
| `0x1007c7ec` | DefaultHandler (infinite loop)                |
| `0x10084d74` | HardFault (infinite loop)                     |
| `0xd0000014` | SIO GPIO_OUT_SET register                     |
| `0xd0000018` | SIO GPIO_OUT_CLR register                     |
| `0x40070000` | UART0 base                                    |
| `0x2000003c` | HEAP_MEM (256 KiB)                            |
| `0x20080000` | Initial stack pointer                         |

---

## 13. Pulley Instruction Set Architecture

### 13.1 Overview

Pulley is Wasmtime's portable bytecode interpreter (wasmtime 43.0.0,
`pulley-interpreter` crate v43.0.0). It defines a register-based ISA
with variable-length instructions, designed for efficient interpretation
rather than native execution.

### 13.2 Encoding Format

**Primary opcodes** use a 1-byte opcode followed by operands:

```
[opcode:1] [operands:0-9]
```

There are **220 primary opcodes** (0x00-0xDB). Opcode `0xDC` is the
**ExtendedOp** sentinel — when the interpreter encounters it, it reads
a 2-byte extended opcode:

```
[0xDC] [ext_opcode:2] [operands:0-N]
```

There are **310 extended opcodes** (0x0000-0x0135) for SIMD, float
conversions, and complex operations.

### 13.3 Register File

Pulley has 32 general-purpose 64-bit registers:

| Register    | Index | Purpose                                  |
| ----------- | ----- | ---------------------------------------- |
| `x0`-`x15`  | 0-15  | Arguments, return values, temporaries    |
| `x16`-`x25` | 16-25 | Callee-saved (pushed by push_frame_save) |
| `x26`-`x29` | 26-29 | Callee-saved                             |
| `sp`        | 30    | Stack pointer                            |
| `spilltmp0` | 31    | Spill temporary (compiler internal)      |

In addition, there are 32 float/vector registers (`f0`-`f31`, 128 bits
each) and a dedicated link register (`lr`) for return addresses.

### 13.4 Key Instructions Used by This Binary

| Opcode | Mnemonic            | Operands           | Description                              |
| ------ | ------------------- | ------------------ | ---------------------------------------- |
| `0x00` | `nop`               | (none)             | No operation                             |
| `0x01` | `ret`               | (none)             | Return to caller (pop lr, branch)        |
| `0x02` | `call`              | `i32 offset`       | Call relative, save lr                   |
| `0x07` | `call_indirect`     | `r`                | Call through register (function pointer) |
| `0x08` | `jump`              | `i32 offset`       | Unconditional relative jump              |
| `0x0a` | `br_if32`           | `r, i32 offset`    | Branch if reg != 0 (32-bit test)         |
| `0x41` | `xmov`              | `r_dst, r_src`     | Copy 64-bit register                     |
| `0x42` | `xzero`             | `r`                | Set register to 0                        |
| `0x44` | `xconst8`           | `r, i8`            | Load sign-extended 8-bit immediate       |
| `0x45` | `xconst16`          | `r, i16`           | Load sign-extended 16-bit immediate      |
| `0x48` | `xadd32`            | `r, r, r`          | 32-bit add                               |
| `0x84` | `xload32le_o32`     | `r, r, i32`        | Load 32-bit LE from base + offset        |
| `0x88` | `xstore32le_o32`    | `r, i32, r`        | Store 32-bit LE to base + offset         |
| `0xa8` | `push_frame`        | (none)             | Save lr, allocate frame                  |
| `0xa9` | `pop_frame`         | (none)             | Restore lr, deallocate frame             |
| `0xaa` | `push_frame_save`   | `u16 amt, bitmask` | Save lr + callee-saved regs, alloc frame |
| `0xab` | `pop_frame_restore` | `u16 amt, bitmask` | Restore callee-saved regs + lr, dealloc  |
| `0xdc` | *(extended prefix)* | `u16 ext_op, ...`  | Extended opcode sentinel                 |

#### Extended Opcodes

| Ext Op   | Mnemonic             | Operands | Description                     |
| -------- | -------------------- | -------- | ------------------------------- |
| `0x0001` | `call_indirect_host` | `u8`     | Call host function by index     |
| `0x0002` | `xpcadd`             | `r, i32` | PC-relative address computation |
| `0x0003` | `xmov_fp`            | `r`      | Move frame pointer to register  |

`call_indirect_host` is the critical instruction for guest-to-host
transitions. The operand byte identifies which host import to call.

---

## 14. Pulley Bytecode Disassembly

### 14.1 Guest::run() — Blink Loop

The guest component's `run()` function compiles to Pulley bytecode that
implements the LED blink loop. The structure is:

```
; function[N]: Guest::run()
; Source: impl Guest for BlinkyApp { fn run() { ... } }

push_frame_save <frame>, <callee-saved regs>

; Load VMContext and function pointers
xload32le_o32 x_heap, x0, 28          ; heap_base
xmov x_vmctx, x0                      ; save VMContext
xload32le_o32 x_set_high_fn, x_vmctx, ...  ; set_high fn ptr
xload32le_o32 x_set_low_fn, x_vmctx, ...   ; set_low fn ptr
xload32le_o32 x_delay_fn, x_vmctx, ...     ; delay_ms fn ptr

.loop:
    ; --- gpio::set_high(25) ---
    xconst8 x2, 25                    ; x2 = pin = 25 (GPIO25)
    xmov x0, x_callee_vmctx
    xmov x1, x_vmctx
    call_indirect x_set_high_fn        ; set_high(25)

    ; --- timing::delay_ms(500) ---
    xconst16 x2, 500                   ; x2 = 500 ms
    xmov x0, x_callee_vmctx
    xmov x1, x_vmctx
    call_indirect x_delay_fn           ; delay_ms(500)

    ; --- gpio::set_low(25) ---
    xconst8 x2, 25                    ; x2 = pin = 25
    xmov x0, x_callee_vmctx
    xmov x1, x_vmctx
    call_indirect x_set_low_fn         ; set_low(25)

    ; --- timing::delay_ms(500) ---
    xconst16 x2, 500                   ; x2 = 500 ms
    xmov x0, x_callee_vmctx
    xmov x1, x_vmctx
    call_indirect x_delay_fn           ; delay_ms(500)

    jump .loop                         ; Infinite loop — never returns
```

**Key observations:**

- Pin 25 is a compile-time constant (`xconst8 x2, 25`) loaded every
  iteration — the compiler does not hoist it out of the loop
- Delay 500 ms is re-loaded each iteration (`xconst16 x2, 500`)
- `call_indirect` through function pointer registers — these are
  loaded from the VMContext component tables
- The function never returns — `jump` creates an infinite loop

### 14.2 panic() — Infinite Loop

```
push_frame
jump 0x0                              ; jump to self (infinite spin)
```

Matches `fn panic(_: &PanicInfo) -> ! { loop { spin_loop() } }`.

---

## 15. Ghidra Analysis Walkthrough

### 15.1 Import and Initial Analysis

1. **File -> Import File**: Select the ELF. Ghidra auto-detects
   `ARM:LE:32:v8T` (ARMv8 Thumb). Accept the defaults.

2. **Auto-analysis**: Click "Yes" when prompted to analyze. Ghidra will:
   - Identify 2,375 functions from the symbol table
   - Resolve Rust-mangled names
   - Detect the ARM vector table at `0x10000000`
   - Find cross-references between functions
   - Identify the `tbh` dispatch table in the Pulley interpreter

3. **Analysis time**: ~30 seconds for this 1.12 MiB binary.

### 15.2 Symbol Tree Navigation

```
Functions/ (2,375 total)
+-- Reset                              0x1000010c
+-- main                               0x10008410
+-- __cortex_m_rt_main                 0x100075a8
+-- embedded_wasm_blinky::run_wasm     0x10007760
+-- led::set_high                      0x1000691c
+-- led::set_low                       0x10006858
+-- uart::write_msg                    0x10007614
+-- pulley_interpreter::interp::Interpreter::run  0x10064ef4
+-- pulley_interpreter::decode::decode_one_extended  0x10068210
+-- wasmtime::runtime::vm::interpreter::InterpreterRef::call  0x1000d7b8
+-- ... (2,362 more)
```

### 15.3 Finding and Extracting the cwasm Blob

1. Navigate to `0x1008a059` in the Listing view
2. Ghidra shows the `7f 45 4c 46` (ELF magic) bytes
3. Right-click -> **Select Bytes** -> enter length 24680 (0x6068)
4. **File -> Export Selection** -> Binary format -> save as `blinky.cwasm`

The extracted file is a valid ELF64 targeting `pulley32-unknown-unknown-elf`.
To analyze the Pulley bytecode inside this blob, use the
[G-Pulley](https://github.com/mytechnotalent/G-Pulley) Ghidra extension.

### 15.4 Ghidra + G-Pulley: Full-Stack Analysis

With the [G-Pulley](https://github.com/mytechnotalent/G-Pulley) extension
installed, Ghidra can analyze **both** the ARM host firmware and the
Pulley guest bytecode:

| Aspect                  | ARM Host Code             | Pulley Guest Code (G-Pulley)        |
| ----------------------- | ------------------------- | ----------------------------------- |
| Disassembly             | Full ARM Thumb-2          | Full Pulley ISA mnemonics           |
| Function identification | Automatic from symbols    | Automatic (cwasm loader + analyzer) |
| Cross-references        | Full xref graph           | Function calls and branches         |
| Control flow            | CFG with switch detection | Branch and jump targets resolved    |
| Host call boundary      | `InterpreterRef::call`    | `call_indirect_host` instructions   |

**G-Pulley provides**:

- Custom ELF loader that extracts the `.cwasm` blob from the firmware
  and loads Pulley bytecode into Ghidra's Listing view
- SLEIGH processor spec for Pulley 32-bit and 64-bit ISA (Wasmtime v43.0.0)
- Post-load analyzer that discovers functions, trampolines, and host calls
- Full opcode decoding for all 220 primary + 310 extended Pulley opcodes

### 15.5 Recommended Ghidra Workflow

1. **Install G-Pulley**: Download from
   [G-Pulley releases](https://github.com/mytechnotalent/G-Pulley/releases).
   In Ghidra: **File -> Install Extensions -> + -> select zip**. Restart.

2. **Analyze the ARM firmware**: Import the ELF. Ghidra auto-detects
   `ARM:LE:32:v8T`. Run auto-analysis. Follow Reset -> main ->
   `__cortex_m_rt_main` -> `run_wasm` for the boot sequence.

3. **Examine host bindings**: Navigate to `led::set_high` and
   `led::set_low` to see the SIO GPIO register writes. Navigate to
   `uart::write_msg` for the UART diagnostic output path.

4. **Trace the interpreter**: Start at `InterpreterRef::call`, follow
   into `Interpreter::run`, examine the `tbh` switch to understand how
   each opcode category is handled.

5. **Analyze the Pulley bytecode**: Import the firmware ELF again using
   G-Pulley's cwasm loader (select "Pulley cwasm" format). G-Pulley
   extracts the embedded cwasm blob, disassembles all Pulley opcodes,
   and identifies guest functions, trampolines, and host calls.

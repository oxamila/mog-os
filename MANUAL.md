# mog-os — User Manual

Everything you need to build, run, test, and contribute to mog-os.

---

## 1. What you get on screen

After booting (UEFI or legacy BIOS) you see a framebuffer banner and:

```
--------------------------------------------------
MOG-OS ready.
[ui] LEFT/RIGHT select, ENTER confirm, R restart, S shutdown
```

Two on-screen buttons: **Restart** and **Shutdown** (amber border =
selected). Status also goes to the serial console (COM1, 115200 8N1) —
on a PC without a serial port the framebuffer alone is enough.

## 2. Keyboard controls (PS/2)

| Key            | Action                          |
|----------------|---------------------------------|
| ← / → or Tab   | move between Restart/Shutdown   |
| Enter          | activate the selected button    |
| R              | restart immediately             |
| S              | shut down immediately           |

- Works with the laptop's **internal PS/2 keyboard** (e.g. Dell Inspiron
  Mini 1018). USB keyboards only work if the BIOS has legacy USB
  keyboard support enabled.

## 3. What the buttons do

- **Shutdown** — ACPI S5 soft-off: parses RSDP → XSDT → FADT to find the
  `PM1a_CNT` port (e.g. `0x604`) and writes `SLP_TYP|SLP_EN`. Tries the
  real-hardware convention (`0x3400`, SLP_TYP=5) then QEMU's (`0x2000`,
  SLP_TYP=0). Serial shows each step: `[acpi] parse: …`,
  `[power] PM1a_CNT 0x604 <- 0x2000`.
- **Restart** — in order: keyboard-controller reset (`0x64`/`0xFE`) →
  chipset reset (`0xCF9`) → triple fault.

## 4. Build & run

Requirements: Rust stable + `x86_64-unknown-none` target, `qemu-system-x86`,
`ovmf`, `mtools`, GNU make.

```sh
make help          # list all targets
make build         # compile the kernel (release)
make image         # bootable disk image → build/mog-os.img
make run           # boot in QEMU (KVM) with display + serial on terminal
make run-bios      # boot via legacy BIOS/SeaBIOS (Dell path)
make run-atom      # boot under TCG emulation (-cpu Denverton)
```

## 5. Tests (all must be green before merging)

| Command             | What it proves                                        |
|---------------------|-------------------------------------------------------|
| `make smoke`        | UEFI/OVMF boot (KVM) — PASS from serial markers       |
| `make smoke-atom`   | UEFI boot under TCG (`-cpu Denverton`) — deterministic|
| `make smoke-bios`   | legacy BIOS/SeaBIOS boot                              |
| `make smoke-power`  | restart **and** shutdown buttons really work          |

`smoke-power` presses the buttons through the QEMU monitor and requires
a **real QEMU exit** plus **ACPI evidence in the serial log** — a crash
or triple-fault reset cannot fake a pass.

Logs land in `build/serial.log`, `build/serial-power.log`,
`build/monitor.log`.

If `smoke` (KVM) fails intermittently on a loaded machine, that's a known
OVMF-side stall, not mog-os — rerun it or use `smoke-atom`.

## 6. Running on real hardware

```sh
make image
sudo dd if=build/mog-os.img of=/dev/sdX bs=4M conv=fsync status=progress
```

- Double-check `/dev/sdX` — `dd` erases the whole device.
- The image is hybrid: BIOS boot + GPT/ESP in one file; the same stick
  works on legacy BIOS and UEFI machines (Secure Boot must be off).
- Re-run `make image` + `dd` after every rebuild.

## 7. How development works now (branch protection)

`main` is **protected** — direct pushes are rejected, even for you:

```sh
git checkout -b feat/my-change     # 1. branch
git commit -am "feat: …"           # 2. commit
git push -u origin feat/my-change  # 3. push
gh pr create                       # 4. open a pull request
gh pr checks 1 --watch             # 5. wait for CI (≈6 min)
gh pr merge 1                      # 6. merge — branch auto-deletes
```

Rules on `main`: PR required, CI ("Build, lint & boot tests") must pass,
branch must be up to date, no force-push, no deletion, conversations
resolved. There is no required reviewer, so you can merge your own PR
once CI is green.

## 8. Repository security (enabled)

- **Secret scanning** + **push protection** — blocks commits containing
  known secrets (API keys, tokens).
- **Dependabot** security updates + alerts for vulnerable dependencies.
- **Actions token is read-only** (`permissions: contents: read`) — a
  compromised action cannot modify the repo.
- **SECURITY.md** — vulnerability reporting policy (`.github/SECURITY.md`).

To finish setup manually (UI toggles that have no API):
**Settings → Code security and analysis** → enable *private vulnerability
reporting* and *validity checks*.

## 9. Troubleshooting

| Symptom | Fix |
|---|---|
| `smoke` fails but `smoke-atom` passes | known OVMF/KVM stall — rerun; use `smoke-atom` for deterministic results |
| `power: FAIL … no ACPI shutdown evidence` | read `build/serial-power.log` — the `[acpi] parse:` lines show where it stopped |
| Buttons don't respond | PS/2 keyboard only; check legacy USB support in BIOS settings |
| No serial output | normal without a cable — the framebuffer is the primary console |
| Image won't boot on UEFI | Secure Boot must be disabled; image must be written with `dd`, not ISO tooling |

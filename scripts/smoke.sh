#!/usr/bin/env bash
# Boot the mochivm UEFI app under QEMU and check its serial output.
#   - with /dev/kvm:   full VMX smoke test (nested virt), expects "guest halted"
#   - without /dev/kvm: boot-only test (TCG can't expose VMX), expects "mochivm booted"
set -euo pipefail

EFI="target/x86_64-unknown-uefi/release/mochivm.efi"
DISK="disk.img"
SERIAL="serial.log"

if [ -e /usr/share/OVMF/OVMF_CODE_4M.fd ]; then
  OVMF_CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
  OVMF_VARS="/usr/share/OVMF/OVMF_VARS_4M.fd"
else
  OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
  OVMF_VARS="/usr/share/OVMF/OVMF_VARS.fd"
fi

rm -f "$DISK" "$SERIAL" OVMF_VARS.fd
cp "$OVMF_VARS" OVMF_VARS.fd

# build a FAT32 image with the app as EFI/BOOT/BOOTX64.EFI
mformat -C -i "$DISK" -H 32 -T 131072 -F ::
mmd -i "$DISK" ::/EFI ::/EFI/BOOT
mcopy -i "$DISK" "$EFI" ::/EFI/BOOT/BOOTX64.EFI

ACCEL_ARGS=()
if [ -e /dev/kvm ]; then
  echo ":: KVM available - running full VMX smoke test"
  ACCEL_ARGS=(-enable-kvm -cpu host)
else
  echo ":: no /dev/kvm - running boot-only smoke test"
  ACCEL_ARGS=(-accel tcg -cpu max)
fi

timeout 20 qemu-system-x86_64 \
  -machine q35 \
  "${ACCEL_ARGS[@]}" \
  -m 512 \
  -smp 1 \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive if=pflash,format=raw,file=OVMF_VARS.fd \
  -drive if=virtio,format=raw,file="$DISK" \
  -serial "file:$SERIAL" \
  -display none \
  -no-reboot \
  -nographic || true

echo ":: serial log:"
cat "$SERIAL" || true

if [ -e /dev/kvm ]; then
  grep -q "guest halted" "$SERIAL"
else
  grep -q "mochivm booted" "$SERIAL"
fi
echo ":: smoke test passed"

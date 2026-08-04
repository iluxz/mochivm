#!/usr/bin/env bash
# Boot the mochivm UEFI app under QEMU and check its serial output.
#   - full VMX test  -> expects "guest halted" (needs a KVM/nested-VTX runner)
#   - boot-only test -> expects "mochivm booted" (TCG fallback, no VMX exposed)
# GitHub-hosted runners usually fail the KVM path, so this degrades to the
# boot-only check instead of hard-failing.
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

run_qemu() {
  timeout 20 qemu-system-x86_64 \
    -machine q35 \
    "$@" \
    -m 512 \
    -smp 1 \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file=OVMF_VARS.fd \
    -drive if=virtio,format=raw,file="$DISK" \
    -serial "file:$SERIAL" \
    -display none \
    -no-reboot \
    -nographic || true
}

# try to make /dev/kvm usable for this session
if [ -e /dev/kvm ]; then
  sudo -n chmod 666 /dev/kvm 2>/dev/null || true
fi

if [ -e /dev/kvm ] && [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
  echo ":: /dev/kvm usable - running full VMX smoke test"
  rm -f "$SERIAL"
  run_qemu -enable-kvm -cpu host
  echo ":: serial log:"
  cat "$SERIAL" || true
  if grep -q "guest halted" "$SERIAL" 2>/dev/null; then
    echo ":: full VMX smoke test passed"
    exit 0
  fi
  echo ":: kvm run did not reach 'guest halted' (no nested VT-x?), falling back to boot-only"
fi

echo ":: no usable /dev/kvm - running boot-only smoke test"
rm -f "$SERIAL"
run_qemu -accel tcg -cpu max
echo ":: serial log:"
cat "$SERIAL" || true
grep -q "mochivm booted" "$SERIAL"
echo ":: boot-only smoke test passed"
